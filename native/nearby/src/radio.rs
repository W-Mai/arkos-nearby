use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::fs::symlink;
use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use crate::content::digest;
use crate::context::INSTALL;
use crate::session::{
    DRIVER_SHA, RoomEntry, SERVICES, State, WPA_SHA, best, cmd, credentials, current, task, update,
};
use crate::system::{mkdir, private_write, random_id};
use crate::{Result, require};

pub struct Control {
    socket: UnixDatagram,
    path: PathBuf,
    events: Vec<String>,
}
impl Control {
    pub fn enrollee(&mut self) -> Result<Option<String>> {
        let _ = self.receive();
        let result = self.events.iter().rev().find_map(|event| {
            event
                .strip_prefix("P2P-PROV-DISC-PBC-REQ ")
                .or_else(|| event.strip_prefix("WPS-ENROLLEE-SEEN "))
                .and_then(|value| value.split_whitespace().next())
                .filter(|mac| crate::compat::valid_mac(mac))
                .map(str::to_owned)
        });
        self.events.retain(|event| {
            !event.starts_with("WPS-ENROLLEE-SEEN ") && !event.starts_with("P2P-PROV-DISC-PBC-REQ ")
        });
        Ok(result)
    }
    pub fn new(state: &State) -> Result<Self> {
        let path = state.root().join(format!("c{}", &random_id()?[..6]));
        let socket = UnixDatagram::bind(&path)?;
        socket.connect(state.root().join("ctrl/wlan0"))?;
        socket.set_read_timeout(Some(Duration::from_millis(200)))?;
        Ok(Self {
            socket,
            path,
            events: vec![],
        })
    }
    pub fn receive(&mut self) -> Result<Option<String>> {
        let mut bytes = [0; 8192];
        let length = self.socket.recv(&mut bytes)?;
        let text = String::from_utf8_lossy(&bytes[..length]).trim().to_owned();
        if text.starts_with('<') {
            if let Some((_, event)) = text.split_once('>') {
                if event.starts_with("WPS-") || event.starts_with("P2P-PROV-DISC-") {
                    eprintln!(
                        "Wireless provisioning event: {}",
                        event.split_whitespace().next().unwrap_or("WPS")
                    );
                }
                self.events.push(event.into());
                if self.events.len() > 128 {
                    self.events.remove(0);
                }
            }
            Ok(None)
        } else {
            Ok(Some(text))
        }
    }
    pub fn request(&mut self, command: &str) -> Result<String> {
        self.socket.send(command.as_bytes())?;
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            match self.receive() {
                Ok(Some(text)) => return Ok(text),
                Ok(None) => {}
                Err(error)
                    if error.downcast_ref::<std::io::Error>().is_some_and(|error| {
                        matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        )
                    }) => {}
                Err(error) => return Err(error),
            }
        }
        Err("Supplicant control timed out".into())
    }
}
impl Drop for Control {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn load_session_driver(state: &State) -> Result<()> {
    let module = Path::new(INSTALL).join("assets/8188eu.ko");
    require(
        digest(&module)? == DRIVER_SHA,
        "Installed wireless module differs",
    )?;
    let swap = !fs::read_to_string("/sys/module/8188eu/version")
        .unwrap_or_default()
        .starts_with("v5.13.3");
    if swap {
        update(&state.id, |state| state.driver_owned = true)?;
        mkdir(Path::new("/run/modprobe.d"))?;
        private_write(
            &Path::new("/run/modprobe.d").join(format!("arkos-nearby-rust-{}.conf", state.id)),
            b"blacklist 8188eu\nblacklist r8188eu\n",
        )?;
    }
    for service in SERVICES {
        cmd(&["systemctl", "stop", service])?;
    }
    if swap {
        cmd(&["modprobe", "-r", "8188eu"])?;
        cmd(&["insmod", module.to_str().unwrap()])?;
    }
    Ok(())
}

fn provisioned_key(value: &str) -> Result<String> {
    let key = if value.starts_with('"') {
        serde_json::from_str::<String>(value)?
    } else {
        value.to_owned()
    };
    require(
        (8..=63).contains(&key.len()) && key.bytes().all(|byte| byte.is_ascii_alphanumeric())
            || key.len() == 64 && key.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Invalid provisioned radio credential",
    )?;
    Ok(key)
}

fn selected_network(room: &RoomEntry, status: &str) -> Option<u32> {
    let fields: std::collections::BTreeMap<_, _> = status
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect();
    if fields.get("wpa_state") != Some(&"COMPLETED")
        || fields.get("ssid") != Some(&room.ssid.as_str())
        || !fields
            .get("bssid")
            .is_some_and(|bssid| bssid.eq_ignore_ascii_case(&room.bssid))
    {
        return None;
    }
    fields.get("id")?.parse().ok()
}

fn saved_credential(room: &RoomEntry, config: &str) -> Result<String> {
    let mut ssid = String::new();
    let mut psk = String::new();
    let mut active = false;
    let mut keys = std::collections::BTreeSet::new();
    for line in config.lines().map(str::trim) {
        if line == "network={" {
            active = true;
            ssid.clear();
            psk.clear();
        } else if active && line == "}" {
            if ssid == serde_json::to_string(&room.ssid)? && !psk.is_empty() {
                keys.insert(provisioned_key(&psk)?);
            }
            active = false;
        } else if active {
            if let Some(value) = line.strip_prefix("ssid=") {
                ssid = value.into();
            } else if let Some(value) = line.strip_prefix("psk=") {
                psk = value.into();
            }
        }
    }
    require(
        keys.len() == 1,
        "No unique credential is bound to the provisioned room",
    )?;
    Ok(keys.into_iter().next().unwrap())
}

fn provision(state: &State) -> Result<String> {
    let mut control = private_supplicant(state, &supplicant_config(state))?;
    require(
        control.request("P2P_FIND 20 type=social")? == "OK",
        "Could not discover the selected Wi-Fi Direct peer",
    )?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        current(&state.id)?;
        let peer = control.request(&format!("P2P_PEER {}", state.room.bssid))?;
        if peer
            .lines()
            .next()
            .is_some_and(|line| line.eq_ignore_ascii_case(&state.room.bssid))
        {
            break;
        }
        require(
            Instant::now() < deadline,
            "The selected Wi-Fi Direct peer was not discovered",
        )?;
        thread::sleep(Duration::from_millis(200));
    }
    require(
        control.request("P2P_STOP_FIND")? == "OK",
        "Could not stop the owned discovery",
    )?;
    let command = format!(
        "P2P_CONNECT {} pbc persistent join provdisc freq=2412 ssid={}",
        state.room.bssid,
        crate::identity::encode(state.room.ssid.as_bytes())
    );
    require(
        control.request(&command)? == "OK",
        "Could not submit selected-room provisioning",
    )?;
    eprintln!("Sent selected-peer Wi-Fi Direct provisioning request");
    crate::session::progress(&state.id, None, "connecting", "等待房主确认加入...")?;
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        current(&state.id)?;
        if selected_network(&state.room, &control.request("STATUS")?).is_some() {
            break;
        }
        require(
            !control.events.iter().any(|event| {
                event.starts_with("P2P-GROUP-FORMATION-FAILURE") || event.starts_with("WPS-FAIL")
            }),
            "Selected-room provisioning failed",
        )?;
        require(
            Instant::now() < deadline,
            "Owner-confirmed wireless provisioning timed out",
        )?;
        thread::sleep(Duration::from_millis(200));
    }
    // GET_NETWORK intentionally hides keys. Save only this private session's persistent group.
    require(
        control.request("SAVE_CONFIG")? == "OK",
        "Could not export owned provisioning credentials",
    )?;
    let key = saved_credential(
        &state.room,
        &fs::read_to_string(state.root().join("go.conf"))?,
    )?;
    eprintln!("Selected-room Wi-Fi Direct provisioning completed");
    crate::session::progress(&state.id, None, "connecting", "正在连接好友...")?;
    receive_client_address(state, &mut control)?;
    Ok(key)
}

fn cached_config(state: &State, key: &str) -> Result<String> {
    let key = provisioned_key(key)?;
    let psk = if key.len() == 64 {
        key
    } else {
        serde_json::to_string(&key)?
    };
    Ok(supplicant_config(state)
        + &format!(
            "network={{\n ssid={}\n bssid={}\n psk={}\n key_mgmt=WPA-PSK\n proto=RSN\n scan_freq=2412\n}}\n",
            serde_json::to_string(&state.room.ssid)?,
            state.room.bssid,
            psk
        ))
}

fn receive_client_address(state: &State, control: &mut Control) -> Result<()> {
    start_client_dhcp(state)?;
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        current(&state.id)?;
        require(
            selected_network(&state.room, &control.request("STATUS")?).is_some(),
            "The selected wireless group disconnected",
        )?;
        if state.root().join("client-address").is_file() {
            break;
        }
        require(
            Instant::now() < deadline,
            "The selected group did not receive a DHCP lease",
        )?;
        thread::sleep(Duration::from_millis(100));
    }
    eprintln!("Selected group retained for gameplay with its DHCP lease");
    Ok(())
}

fn connect_cached(state: &State, key: &str) -> Result<()> {
    let mut control = private_supplicant(state, &cached_config(state, key)?)?;
    let deadline = Instant::now() + Duration::from_secs(25);
    while selected_network(&state.room, &control.request("STATUS")?).is_none() {
        current(&state.id)?;
        require(
            Instant::now() < deadline,
            "Could not connect to the selected remembered room",
        )?;
        thread::sleep(Duration::from_millis(100));
    }
    eprintln!("Connected to selected remembered room through the private nl80211 radio");
    receive_client_address(state, &mut control)
}

fn lease_address(address: &str, server: &str, mask: &str) -> Result<std::net::Ipv4Addr> {
    let address: std::net::Ipv4Addr = address.parse()?;
    let bytes = address.octets();
    require(
        bytes[..3] == [192, 168, 49]
            && (10..=20).contains(&bytes[3])
            && server == "192.168.49.1"
            && mask == "255.255.255.0",
        "DHCP lease is outside the owned room",
    )?;
    Ok(address)
}

pub fn dhcp_hook() -> Result<()> {
    let reason = std::env::var("reason").unwrap_or_default();
    if !matches!(reason.as_str(), "BOUND" | "RENEW" | "REBIND" | "REBOOT") {
        return Ok(());
    }
    let id = std::env::var("ARKOS_NEARBY_DHCP_SESSION")?;
    let state = current(&id)?;
    require(
        unsafe { libc::geteuid() } == 0
            && state.role == "client"
            && !state.mounts.is_empty()
            && std::env::var("interface")? == "wlan0"
            && fs::read_to_string("/sys/class/net/wlan0/address")?.trim() == state.self_mac,
        "DHCP callback does not own this interface",
    )?;
    let address = lease_address(
        &std::env::var("new_ip_address")?,
        &std::env::var("new_dhcp_server_identifier")?,
        &std::env::var("new_subnet_mask")?,
    )?;
    cmd(&[
        "ip",
        "address",
        "replace",
        &format!("{address}/24"),
        "dev",
        "wlan0",
    ])?;
    private_write(
        &state.root().join("client-address"),
        address.to_string().as_bytes(),
    )?;
    Ok(())
}

fn start_client_dhcp(state: &State) -> Result<()> {
    let root = state.root();
    private_write(&root.join("dhclient.conf"), format!("timeout 20;\nretry 2;\ninitial-interval 1;\nrequest subnet-mask, dhcp-server-identifier, dhcp-lease-time;\ninterface \"wlan0\" {{ send dhcp-client-identifier 01:{}; }}\n",state.self_mac).as_bytes())?;
    symlink(
        Path::new(INSTALL).join("arkos-nearby"),
        root.join("dhcp-hook"),
    )?;
    let args = vec![
        "--collect".into(),
        format!("--unit={}-dhcp", state.unit()),
        "--property=StandardOutput=null".into(),
        "--property=StandardError=journal".into(),
        "/usr/sbin/dhclient".into(),
        "-4".into(),
        "-d".into(),
        "-1".into(),
        "-pf".into(),
        root.join("dhclient.pid").to_string_lossy().into_owned(),
        "-lf".into(),
        root.join("dhclient.leases").to_string_lossy().into_owned(),
        "-cf".into(),
        root.join("dhclient.conf").to_string_lossy().into_owned(),
        "-sf".into(),
        root.join("dhcp-hook").to_string_lossy().into_owned(),
        "-e".into(),
        format!("ARKOS_NEARBY_DHCP_SESSION={}", state.id),
        "wlan0".into(),
    ];
    crate::system::command(
        "systemd-run",
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        15,
        true,
    )?;
    Ok(())
}

fn private_supplicant(state: &State, config: &str) -> Result<Control> {
    let assets = Path::new(INSTALL).join("assets");
    require(
        digest(&assets.join("8188eu.ko"))? == DRIVER_SHA
            && digest(&assets.join("wpa_supplicant"))? == WPA_SHA,
        "Installed wireless assets differ",
    )?;
    let root = state.root();
    let binary = root.join("wpa_supplicant");
    private_write(&binary, b"")?;
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))?;
    private_write(&root.join("go.conf"), config.as_bytes())?;
    cmd(&["nmcli", "radio", "wifi", "on"])?;
    load_session_driver(state)?;
    update(&state.id, |state| state.mounts = vec![binary.clone()])?;
    cmd(&[
        "mount",
        "--bind",
        assets.join("wpa_supplicant").to_str().unwrap(),
        binary.to_str().unwrap(),
    ])?;
    cmd(&[
        "mount",
        "-o",
        "remount,bind,ro,exec,nosuid,nodev",
        binary.to_str().unwrap(),
    ])?;
    cmd(&["ip", "link", "set", "wlan0", "up"])?;
    let mut args = vec![
        "--collect".into(),
        format!("--unit={}-private", state.unit()),
        "--property=StandardOutput=null".into(),
        "--property=StandardError=null".into(),
        "--property=UMask=0077".into(),
        format!(
            "--setenv=ARKOS_RTL_IE_BRIDGE={}",
            if state.role == "host" { 1 } else { 0 }
        ),
        format!("--setenv=ARKOS_RTL_IE_JOB={}", &state.id[..12]),
        format!("--setenv=ARKOS_RTL_IE_MAC={}", state.self_mac),
        format!(
            "--setenv=ARKOS_RTL_IE_REPORT={}",
            root.join("rtl-ie.json").display()
        ),
        binary.to_string_lossy().into_owned(),
        "-Dnl80211".into(),
        "-iwlan0".into(),
        format!("-c{}", root.join("go.conf").display()),
    ];
    crate::system::command(
        "systemd-run",
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        15,
        true,
    )?;
    args.clear();
    let deadline = Instant::now() + Duration::from_secs(8);
    while !root.join("ctrl/wlan0").exists() && Instant::now() < deadline {
        current(&state.id)?;
        thread::sleep(Duration::from_millis(100));
    }
    let mut control = Control::new(state)?;
    require(
        control.request("ATTACH")? == "OK",
        "Could not subscribe to group events",
    )?;
    Ok(control)
}

fn supplicant_config(state: &State) -> String {
    format!(
        "ctrl_interface={}\nupdate_config={}\np2p_no_group_iface=1\np2p_group_idle=-1\nmax_num_sta=2\ndevice_name=ArkOS-Nearby\nconfig_methods=push_button\ndevice_type=1-0050F204-1\n",
        state.root().join("ctrl").display(),
        if state.role == "client" { 1 } else { 0 }
    )
}

pub fn host(state: &State) -> Result<Control> {
    let config = supplicant_config(state)
        + &format!(
            "network={{\n ssid={}\n psk={}\n mode=3\n disabled=2\n frequency=2412\n}}\n",
            serde_json::to_string(&state.room.ssid)?,
            serde_json::to_string(&credentials()?["psk"])?
        );
    let mut control = private_supplicant(state, &config)?;
    require(
        control.request("P2P_GROUP_ADD persistent=0 freq=2412")? == "OK",
        "Could not create selected group",
    )?;
    let deadline = Instant::now() + Duration::from_secs(8);
    while !control
        .events
        .iter()
        .any(|event| event.starts_with("P2P-GROUP-STARTED wlan0 GO "))
    {
        require(
            Instant::now() < deadline,
            "Wireless group did not become ready",
        )?;
        current(&state.id)?;
        let _ = control.receive();
    }
    require(
        control.events.iter().any(|event| {
            event.starts_with("P2P-GROUP-STARTED wlan0 GO ")
                && event.contains(&format!("ssid=\"{}\"", state.room.ssid))
        }),
        "Created group differs from selected room",
    )?;
    cmd(&["ip", "address", "add", "192.168.49.1/24", "dev", "wlan0"])?;
    Ok(control)
}

pub fn services(state: &State) -> Result<()> {
    let root = state.root();
    let args = vec![
        "--collect".into(),
        format!("--unit={}-dhcp", state.unit()),
        "--property=StandardOutput=null".into(),
        "--property=StandardError=null".into(),
        "/usr/sbin/dnsmasq".into(),
        "--no-daemon".into(),
        "--conf-file=/dev/null".into(),
        "--port=0".into(),
        "--interface=wlan0".into(),
        "--bind-interfaces".into(),
        "--listen-address=192.168.49.1".into(),
        "--dhcp-range=192.168.49.10,192.168.49.20,255.255.255.0,5m".into(),
        "--dhcp-option=3".into(),
        "--dhcp-option=6".into(),
        "--no-ping".into(),
        "--dhcp-authoritative".into(),
        format!("--dhcp-leasefile={}", root.join("leases").display()),
        format!("--pid-file={}", root.join("dnsmasq.pid").display()),
        "--user=root".into(),
    ];
    crate::system::command(
        "systemd-run",
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        15,
        true,
    )?;
    task(state, "-http", "serve", &[])?;
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if crate::http::request("/health", None).is_ok() {
            break;
        }
        require(Instant::now() < deadline, "Room HTTP self-check failed")?;
        thread::sleep(Duration::from_millis(100));
    }
    require(
        best(&[
            "systemctl",
            "is-active",
            &format!("{}-dhcp.service", state.unit()),
        ]) == "active",
        "DHCP service did not start",
    )?;
    Ok(())
}

pub fn join(state: &State) -> Result<()> {
    let remembered = crate::identity::friends()?
        .values()
        .find(|friend| friend.last_mac == state.room.bssid)
        .and_then(|friend| friend.wireless_key.clone())
        .or_else(|| {
            credentials()
                .ok()
                .filter(|config| config["manual_fallback"] == true)
                .and_then(|config| config["psk"].as_str().map(str::to_owned))
        });
    let key = if let Some(key) = remembered {
        connect_cached(state, &key)?;
        provisioned_key(&key)?
    } else {
        provision(state)?
    };
    private_write(&state.root().join("provisioned-key"), key.as_bytes())?;
    Ok(())
}

pub fn restore(state: &State) -> Result<()> {
    let mut errors = Vec::new();
    let mut attempt = |name: &str, result: Result<String>| {
        if let Err(error) = result {
            errors.push(format!("{name}: {error}"));
        }
    };
    if state.role == "host" || !state.mounts.is_empty() {
        for suffix in ["-http", "-dhcp", "-private"] {
            best(&[
                "systemctl",
                "stop",
                &format!("{}{suffix}.service", state.unit()),
            ]);
        }
    }
    if state.role == "host" {
        best(&["ip", "address", "del", "192.168.49.1/24", "dev", "wlan0"]);
    } else if let Ok(address) = fs::read_to_string(state.root().join("client-address")) {
        let address = lease_address(&address, "192.168.49.1", "255.255.255.0")?;
        best(&[
            "ip",
            "address",
            "del",
            &format!("{address}/24"),
            "dev",
            "wlan0",
        ]);
    }
    if !state.mounts.is_empty() {
        best(&["ip", "link", "set", "wlan0", "down"]);
        best(&["iw", "dev", "wlan0", "set", "type", "managed"]);
        best(&["ip", "link", "set", "wlan0", "up"]);
        for mount in &state.mounts {
            if best(&["mountpoint", "-q", mount.to_str().unwrap()]).is_empty() {
                let output = crate::system::command(
                    "findmnt",
                    &["-n", "-o", "TARGET", "--target", mount.to_str().unwrap()],
                    4,
                    false,
                )
                .unwrap_or_default();
                if output == mount.to_str().unwrap() {
                    attempt(
                        "owned binary mount",
                        cmd(&["umount", mount.to_str().unwrap()]),
                    );
                }
            }
        }
    }
    if state.role == "client" && state.root().join("nm-profile").is_file() {
        best(&["nmcli", "connection", "delete", &state.profile()]);
    }
    if state.driver_owned {
        for service in SERVICES {
            best(&["systemctl", "stop", service]);
        }
        if Path::new("/sys/module/8188eu").exists() {
            attempt("session driver", cmd(&["modprobe", "-r", "8188eu"]));
        }
        let blacklist =
            Path::new("/run/modprobe.d").join(format!("arkos-nearby-rust-{}.conf", state.id));
        if blacklist.exists() {
            fs::remove_file(blacklist)?;
        }
        attempt("original driver", cmd(&["modprobe", "8188eu"]));
    }
    if state.role == "host" || state.driver_owned || !state.mounts.is_empty() {
        for (index, service) in SERVICES.iter().enumerate().rev() {
            if state.services_before[index] {
                attempt(service, cmd(&["systemctl", "start", service]));
            }
        }
    }
    for file in [
        "password",
        "go.conf",
        "start-game",
        "provisioned-key",
        "wps-name",
        "nm-profile",
        "client-address",
        "driver.ko",
        "wpa_supplicant",
    ] {
        let path = state.root().join(file);
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    require(errors.is_empty(), &errors.join("; "))
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provisioning_events_and_identity_approval_have_distinct_authority() {
        let (socket, peer) = UnixDatagram::pair().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(1)))
            .unwrap();
        let mut control = Control {
            socket,
            path: PathBuf::new(),
            events: vec![],
        };
        peer.send(b"<3>P2P-PROV-DISC-PBC-REQ 02:aa:bb:cc:dd:01 name='Peer'")
            .unwrap();
        assert_eq!(
            control.enrollee().unwrap(),
            Some("02:aa:bb:cc:dd:01".into())
        );
        assert!(control.enrollee().unwrap().is_none());
        peer.send(b"<3>P2P-DEVICE-FOUND 02:aa:bb:cc:dd:02").unwrap();
        assert!(control.enrollee().unwrap().is_none());
        assert!(
            crate::session::PairingRequest::Identity
                .radio_command("02:aa:bb:cc:dd:01")
                .is_none()
        );
        assert_eq!(
            crate::session::PairingRequest::ProvisionDiscovery
                .radio_command("02:aa:bb:cc:dd:01")
                .unwrap(),
            "WPS_PBC p2p_dev_addr=02:aa:bb:cc:dd:01"
        );
    }

    #[test]
    fn dhcp_callbacks_can_only_apply_the_owned_room_subnet() {
        assert!(lease_address("192.168.49.10", "192.168.49.1", "255.255.255.0").is_ok());
        for (address, server, mask) in [
            ("192.168.1.10", "192.168.49.1", "255.255.255.0"),
            ("192.168.49.1", "192.168.49.1", "255.255.255.0"),
            ("192.168.49.10", "192.168.1.1", "255.255.255.0"),
            ("192.168.49.10", "192.168.49.1", "0.0.0.0"),
        ] {
            assert!(lease_address(address, server, mask).is_err());
        }
    }

    #[test]
    fn provisioning_cannot_handoff_another_group_or_unfinished_connection() {
        let room = RoomEntry::observed(
            "DIRECT-NP-aaaaaa-1111111111111",
            "02:aa:bb:cc:dd:01",
            80,
            &"a".repeat(64),
        )
        .unwrap();
        let status = format!(
            "wpa_state=COMPLETED\nssid={}\nbssid={}\nid=3\n",
            room.ssid, room.bssid
        );
        assert_eq!(selected_network(&room, &status), Some(3));
        for other in [
            status.replace("COMPLETED", "SCANNING"),
            status.replace("dd:01", "dd:02"),
            status.replace("1111111111111", "2222222222222"),
        ] {
            assert_eq!(selected_network(&room, &other), None);
        }
        assert_eq!(
            provisioned_key(&format!("\"{}\"", "a".repeat(32))).unwrap(),
            "a".repeat(32)
        );
        assert_eq!(provisioned_key(&"a".repeat(64)).unwrap(), "a".repeat(64));
        assert!(provisioned_key("FAIL").is_err());
        assert!(provisioned_key(&"g".repeat(64)).is_err());
        let config = format!(
            "network={{\nssid=\"{}\"\npsk={}\n}}\n",
            room.ssid,
            "a".repeat(64)
        );
        assert_eq!(saved_credential(&room, &config).unwrap(), "a".repeat(64));
        assert!(
            saved_credential(&room, &config.replace("1111111111111", "2222222222222")).is_err()
        );
        assert!(
            saved_credential(
                &room,
                &(config.clone() + &config.replace(&"a".repeat(64), &"b".repeat(64)))
            )
            .is_err()
        );
    }
}
