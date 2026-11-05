use super::{Access, Driver, Snapshot};
use crate::{Result, require, system};
use nearby_menu::wifi::{Info, Network, Secret, Security, valid_ssid};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const JOURNAL: &str = "/run/arkos-nearby-rust/wifi-operation.json";
pub(super) struct NetworkManager;
impl Driver for NetworkManager {
    fn snapshot(&mut self, refresh: bool) -> Result<Snapshot> {
        crate::session::try_while_idle(|| NetworkManager::snapshot(self, refresh))
    }
    fn enable(&mut self) -> Result<Snapshot> {
        crate::session::try_while_idle(|| NetworkManager::enable(self))
    }
    fn disconnect(&mut self, target: &Access, forget: bool) -> Result<Snapshot> {
        crate::session::try_while_idle(|| NetworkManager::disconnect(self, target, forget))
    }
    fn connect(
        &mut self,
        target: &Access,
        secret: &Secret,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u8),
    ) -> Result<Snapshot> {
        crate::session::try_while_idle(|| {
            NetworkManager::connect(self, target, secret, cancel, progress)
        })
    }
}
fn nm(args: &[&str]) -> Result<String> {
    let mut environment = vec!["LC_ALL=C", "nmcli"];
    environment.extend_from_slice(args);
    let seconds = if args.first() == Some(&"--wait") {
        args.get(1)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(4)
            .min(30)
            + 4
    } else {
        7
    };
    system::command("env", &environment, seconds, true)
}
fn run(args: &[&str]) -> Result<()> {
    nm(args).map(|_| ())
}
fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

// nmcli's terse fields escape separators and backslashes independently.
fn fields(line: &str) -> Vec<String> {
    let mut values = vec![String::new()];
    let mut escaped = false;
    for ch in line.chars() {
        if escaped {
            values.last_mut().unwrap().push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == ':' {
            values.push(String::new());
        } else {
            values.last_mut().unwrap().push(ch);
        }
    }
    if escaped {
        values.last_mut().unwrap().push('\\');
    }
    values
}
fn security(value: &str) -> Security {
    if value.contains("802.1X") {
        Security::Enterprise
    } else if value.contains("WPA2") || value.contains("WPA1") || value == "WPA" {
        Security::Psk
    } else if value.is_empty() || value == "--" {
        Security::Open
    } else {
        Security::Unsupported
    }
}
fn key(ssid: &str, security: Security) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("wlan0\0{security:?}\0{ssid}").as_bytes())
    )
}
fn properties() -> Result<BTreeMap<String, String>> {
    let output = nm(&[
        "-t",
        "-e",
        "yes",
        "-f",
        "GENERAL.STATE,GENERAL.REASON,GENERAL.CON-UUID,GENERAL.CONNECTION,IP4.ADDRESS,IP4.GATEWAY,IP6.ADDRESS",
        "device",
        "show",
        "wlan0",
    ])?;
    Ok(output
        .lines()
        .filter_map(|line| {
            let pair = fields(line);
            (pair.len() == 2).then(|| (pair[0].clone(), pair[1].clone()))
        })
        .collect())
}
fn active() -> Result<Option<String>> {
    let value = nm(&["-g", "GENERAL.CON-UUID", "device", "show", "wlan0"])?;
    Ok(uuid(&value).then_some(value))
}
fn profiles() -> Result<Vec<(String, String, Security)>> {
    let list = nm(&["-t", "-e", "yes", "-f", "UUID,TYPE", "connection", "show"])?;
    let mut result = Vec::new();
    for row in list
        .lines()
        .map(fields)
        .filter(|row| row.len() == 2 && row[1] == "802-11-wireless")
        .take(64)
    {
        if !uuid(&row[0]) {
            continue;
        }
        let output = nm(&[
            "-t",
            "-e",
            "yes",
            "-f",
            "802-11-wireless.ssid,802-11-wireless-security.key-mgmt",
            "connection",
            "show",
            "uuid",
            &row[0],
        ])?;
        let values: Vec<_> = output.lines().map(fields).collect();
        let ssid = values
            .iter()
            .find(|r| r.first().is_some_and(|s| s == "802-11-wireless.ssid"))
            .and_then(|r| r.get(1));
        let auth = values
            .iter()
            .find(|r| {
                r.first()
                    .is_some_and(|s| s == "802-11-wireless-security.key-mgmt")
            })
            .and_then(|r| r.get(1))
            .map(String::as_str)
            .unwrap_or("");
        if let Some(ssid) = ssid {
            let security = match auth {
                "wpa-psk" => Security::Psk,
                "wpa-eap" | "ieee8021x" => Security::Enterprise,
                "" | "--" => Security::Open,
                _ => Security::Unsupported,
            };
            result.push((row[0].clone(), ssid.clone(), security));
        }
    }
    Ok(result)
}

impl NetworkManager {
    fn snapshot(&mut self, refresh: bool) -> Result<Snapshot> {
        let radio = nm(&["radio", "wifi"])? == "enabled";
        if !radio {
            return Ok(Snapshot {
                radio,
                ..Default::default()
            });
        }
        if refresh {
            run(&["--wait", "5", "device", "wifi", "rescan", "ifname", "wlan0"])?;
        }
        let output = nm(&[
            "-t",
            "-e",
            "yes",
            "-f",
            "IN-USE,SSID,SIGNAL,SECURITY",
            "device",
            "wifi",
            "list",
            "ifname",
            "wlan0",
            "--rescan",
            "no",
        ])?;
        let saved = profiles()?;
        let current = active()?;
        let mut rows = BTreeMap::<String, Access>::new();
        for values in output.lines().map(fields) {
            if values.len() != 4 || !valid_ssid(&values[1]) {
                continue;
            }
            let security = security(&values[3]);
            let id = key(&values[1], security);
            let connected = values[0] == "*";
            let profile = saved
                .iter()
                .filter(|(_, ssid, auth)| ssid == &values[1] && *auth == security)
                .min_by_key(|(id, _, _)| Some(id) != current.as_ref())
                .map(|(id, _, _)| id.clone());
            let entry = Access {
                network: Network {
                    key: id.clone(),
                    ssid: values[1].clone(),
                    security,
                    signal: values[2].parse::<u8>().unwrap_or(0).min(100),
                    saved: profile.is_some(),
                    connected,
                },
                profile,
                hidden: false,
            };
            rows.entry(id)
                .and_modify(|old| {
                    old.network.connected |= connected;
                    old.network.signal = old.network.signal.max(entry.network.signal);
                })
                .or_insert(entry);
        }
        let mut rows: Vec<_> = rows.into_values().collect();
        rows.sort_by(|a, b| {
            b.network
                .connected
                .cmp(&a.network.connected)
                .then(b.network.signal.cmp(&a.network.signal))
                .then(a.network.key.cmp(&b.network.key))
        });
        let status = properties()?;
        let info = rows.iter().find(|r| r.network.connected).map(|r| Info {
            ssid: r.network.ssid.clone(),
            signal: r.network.signal,
            security: r.network.security,
            address: status
                .get("IP4.ADDRESS[1]")
                .or_else(|| status.get("IP6.ADDRESS[1]"))
                .map(|v| v.split('/').next().unwrap_or(v).into())
                .unwrap_or_default(),
            gateway: status.get("IP4.GATEWAY").cloned().unwrap_or_default(),
        });
        Ok(Snapshot {
            radio,
            rows,
            info,
            uuid: current,
        })
    }
    fn enable(&mut self) -> Result<Snapshot> {
        run(&["radio", "wifi", "on"])?;
        self.snapshot(true)
    }
    fn disconnect(&mut self, target: &Access, forget: bool) -> Result<Snapshot> {
        let profile = target.profile.as_ref().ok_or("该网络尚未保存")?;
        require(uuid(profile), "Invalid Wi-Fi profile")?;
        if active()?.as_ref() == Some(profile) {
            run(&["--wait", "8", "device", "disconnect", "wlan0"])?;
        }
        if forget {
            run(&["connection", "delete", "uuid", profile])?;
        }
        self.snapshot(false)
    }
    fn connect(
        &mut self,
        target: &Access,
        secret: &Secret,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u8),
    ) -> Result<Snapshot> {
        require(valid_ssid(&target.network.ssid), "网络名称无效")?;
        require(
            target.profile.is_some()
                || matches!(target.network.security, Security::Open | Security::Psk),
            "此网络需要预先配置认证信息",
        )?;
        let token = system::random_id()?;
        let original = active()?;
        let radio = nm(&["radio", "wifi"])? == "enabled";
        let fresh = target.profile.is_none() || !secret.value().is_empty();
        let owned = if fresh {
            Some(format!(
                "{}-{}-{}-{}-{}",
                &token[..8],
                &token[8..12],
                &token[12..16],
                &token[16..20],
                &token[20..]
            ))
        } else {
            None
        };
        let profile = owned
            .as_ref()
            .or(target.profile.as_ref())
            .ok_or("Missing profile")?;
        let record = Journal {
            token: token.clone(),
            previous: original,
            profile: profile.clone(),
            owned: owned.is_some(),
            radio,
            committed: false,
        };
        let mut transaction = Transaction::begin(record)?;
        let result = (|| -> Result<Snapshot> {
            if fresh {
                let name = format!("nearby-wifi-{token}");
                let mut args = vec![
                    "connection",
                    "add",
                    "type",
                    "wifi",
                    "ifname",
                    "wlan0",
                    "con-name",
                    &name,
                    "ssid",
                    &target.network.ssid,
                    "connection.uuid",
                    profile,
                    "connection.autoconnect",
                    "no",
                    "802-11-wireless.hidden",
                    if target.hidden { "yes" } else { "no" },
                ];
                if target.network.security == Security::Psk {
                    args.extend(["wifi-sec.key-mgmt", "wpa-psk", "wifi-sec.psk-flags", "0"]);
                }
                run(&args)?;
            }
            let password = transaction.directory.join("credentials");
            let mut args = vec![
                "--wait",
                "30",
                "connection",
                "up",
                "uuid",
                profile,
                "ifname",
                "wlan0",
            ];
            let pass_path = password.to_string_lossy();
            if !secret.value().is_empty() {
                require(
                    nearby_menu::wifi::valid_psk(secret.value()),
                    "密码格式不正确",
                )?;
                system::private_write(
                    &password,
                    format!("802-11-wireless-security.psk:{}\n", secret.value()).as_bytes(),
                )?;
                args.extend(["passwd-file", &pass_path]);
            }
            let mut child = crate::menu::OwnedChild(
                Command::new("nmcli")
                    .args(&args)
                    .env("LC_ALL", "C")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?,
            );
            let deadline = Instant::now() + Duration::from_secs(34);
            let mut observed = Instant::now();
            let status = loop {
                if cancel.load(Ordering::Relaxed) || Instant::now() >= deadline {
                    let _ = child.0.kill();
                    let _ = child.0.wait();
                    return Err(if cancel.load(Ordering::Relaxed) {
                        "连接已取消"
                    } else {
                        "连接超时"
                    }
                    .into());
                }
                if let Some(status) = child.0.try_wait()? {
                    break status;
                }
                if observed.elapsed() >= Duration::from_millis(500) {
                    if let Ok(status) = properties() {
                        let number = status
                            .get("GENERAL.STATE")
                            .and_then(|v| v.split_whitespace().next())
                            .and_then(|v| v.parse::<u16>().ok())
                            .unwrap_or(0);
                        if (70..=100).contains(&number) {
                            progress(1);
                        }
                    }
                    observed = Instant::now();
                }
                std::thread::sleep(Duration::from_millis(30));
            };
            require(status.success(), "未能连接，请检查密码或信号")?;
            let snapshot = self.snapshot(false)?;
            require(!cancel.load(Ordering::Relaxed), "连接已取消")?;
            require(
                snapshot.uuid.as_ref() == Some(profile)
                    && snapshot
                        .info
                        .as_ref()
                        .is_some_and(|i| !i.address.is_empty()),
                "网络尚未取得地址",
            )?;
            if fresh {
                run(&[
                    "connection",
                    "modify",
                    "uuid",
                    profile,
                    "connection.autoconnect",
                    "yes",
                ])?;
            }
            require(!cancel.load(Ordering::Relaxed), "连接已取消")?;
            progress(2);
            transaction.commit()?;
            Ok(snapshot)
        })();
        let _ = fs::remove_file(transaction.directory.join("credentials"));
        if result.is_err() {
            progress(3);
            transaction.restore()?;
        }
        result
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    token: String,
    previous: Option<String>,
    profile: String,
    owned: bool,
    radio: bool,
    committed: bool,
}
struct Transaction {
    record: Journal,
    directory: std::path::PathBuf,
    finished: bool,
}
impl Transaction {
    fn begin(record: Journal) -> Result<Self> {
        require(!Path::new(JOURNAL).exists(), "上次网络操作仍待恢复")?;
        let directory = Path::new(crate::session::BASE).join(format!("wifi-{}", record.token));
        system::mkdir(&directory)?;
        system::write(Path::new(JOURNAL), &record)?;
        let unit = format!("nearby-wifi-{}", record.token);
        let delay = system::command(
            "systemd-run",
            &[
                "--collect",
                &format!("--unit={unit}"),
                "--on-active=55s",
                &format!("{}/arkos-nearby", crate::context::INSTALL),
                "wifi-recover",
                &record.token,
            ],
            5,
            true,
        );
        if delay.is_err() {
            let _ = fs::remove_file(JOURNAL);
            let _ = fs::remove_dir_all(&directory);
        }
        delay?;
        Ok(Self {
            record,
            directory,
            finished: false,
        })
    }
    fn finish(&mut self) -> Result<()> {
        fs::remove_file(JOURNAL)?;
        let _ = system::command(
            "systemctl",
            &["stop", &format!("nearby-wifi-{}.timer", self.record.token)],
            5,
            false,
        );
        fs::remove_dir_all(&self.directory)?;
        self.finished = true;
        Ok(())
    }
    fn commit(&mut self) -> Result<()> {
        self.record.committed = true;
        system::write(Path::new(JOURNAL), &self.record)?;
        self.finish()
    }
    fn restore(&mut self) -> Result<()> {
        let current = active()?;
        if !self.record.committed
            && (current.is_none() || current.as_ref() == Some(&self.record.profile))
        {
            if let Some(previous) = self.record.previous.as_ref() {
                run(&[
                    "--wait",
                    "12",
                    "connection",
                    "up",
                    "uuid",
                    previous,
                    "ifname",
                    "wlan0",
                ])?;
            } else if current.is_some() {
                run(&["--wait", "8", "device", "disconnect", "wlan0"])?;
            }
            if !self.record.radio {
                run(&["radio", "wifi", "off"])?;
            }
        }
        if !self.record.committed
            && self.record.owned
            && nm(&["-g", "UUID", "connection", "show"])?
                .lines()
                .any(|id| id == self.record.profile)
        {
            run(&["connection", "delete", "uuid", &self.record.profile])?;
        }
        self.finish()
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.restore();
        }
    }
}
pub(super) fn pending() -> bool {
    Path::new(JOURNAL).exists()
}
pub(super) fn pending_token() -> Result<Option<String>> {
    if !pending() {
        return Ok(None);
    }
    let journal: Journal = system::read(Path::new(JOURNAL))?;
    Ok(Some(journal.token))
}
pub(super) fn recover(token: &str) -> Result<()> {
    if !Path::new(JOURNAL).exists() {
        return Ok(());
    }
    let record: Journal = system::read(Path::new(JOURNAL))?;
    require(
        crate::protocol::hex(&record.token, 32)
            && record.token == token
            && uuid(&record.profile)
            && record.previous.as_ref().is_none_or(|v| uuid(v)),
        "Invalid Wi-Fi recovery ownership",
    )?;
    let directory = Path::new(crate::session::BASE).join(format!("wifi-{}", record.token));
    Transaction {
        record,
        directory,
        finished: false,
    }
    .restore()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escaped_ssids_and_security_have_stable_distinct_identities() {
        assert_eq!(
            fields(r"*:a\:b\\'c:88:WPA2"),
            vec!["*", "a:b\\'c", "88", "WPA2"]
        );
        assert_ne!(key("same", Security::Open), key("same", Security::Psk));
        assert_eq!(security("WPA2 802.1X"), Security::Enterprise);
        assert_eq!(security("--"), Security::Open);
        assert!(!uuid("--help"));
    }
}
