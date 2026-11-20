//! Presentation metadata carried by the group's WPS probe response.
use crate::catalog::{Index, IndexJob, portable_key};
use crate::context::Context;
use crate::session::{RoomEntry, State};
use crate::system::{command, read};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listing {
    pub owner: String,
    pub game: String,
    pub artwork: String,
}

// These WPS fields are plain config lines with a maximum of 32 UTF-8 bytes.
fn field(text: &str) -> String {
    let mut value = String::new();
    for c in text.chars() {
        if value.len() + c.len_utf8() > 32 {
            break;
        }
        value.push(if c.is_control() || matches!(c, '#' | '"') {
            ' '
        } else {
            c
        });
    }
    value.trim().to_owned()
}

pub(crate) fn host_fields(state: &State) -> String {
    let owner = crate::identity::local_name().unwrap_or_else(|| howff_menu::brand::NAME.into());
    let context = read::<Context>(&state.root().join("context.json")).ok();
    let game = context.as_ref().map_or("", |c| c.title.as_str());
    let key = context
        .as_ref()
        .and_then(|c| portable_key(&c.game_path, &crate::platform::content::roots().ok()?));
    format!(
        "device_name={}\nmanufacturer=Howff\nmodel_name={}\nmodel_number={}\nserial_number={}\n",
        field(&owner),
        field(if game.is_empty() { "Howff" } else { game }),
        key.as_ref()
            .map(|key| crate::identity::encode(key))
            .as_deref()
            .unwrap_or("network"),
        state.room.token
    )
}

#[derive(Default)]
struct Advert {
    owner: String,
    game: String,
    key: String,
    room: String,
    manufacturer: String,
}

fn parse_wps(ies: &[u8]) -> Option<Advert> {
    let mut bytes = ies;
    let mut attributes = Vec::new();
    while !bytes.is_empty() {
        let header = bytes.get(..2)?;
        let end = 2 + usize::from(header[1]);
        let data = bytes.get(2..end)?;
        if header[0] == 221
            && let Some(wps) = data.strip_prefix(&[0, 0x50, 0xf2, 4])
        {
            attributes.extend_from_slice(wps);
        }
        bytes = &bytes[end..];
    }
    let mut data = attributes.as_slice();
    let mut ad = Advert::default();
    while !data.is_empty() {
        let header = data.get(..4)?;
        let end = 4 + usize::from(u16::from_be_bytes([header[2], header[3]]));
        let value = data.get(4..end)?;
        let target = match u16::from_be_bytes([header[0], header[1]]) {
            0x1011 => Some(&mut ad.owner),
            0x1021 => Some(&mut ad.manufacturer),
            0x1023 => Some(&mut ad.game),
            0x1024 => Some(&mut ad.key),
            0x1042 => Some(&mut ad.room),
            _ => None,
        };
        if let Some(target) = target {
            let value = std::str::from_utf8(value).ok()?.trim_end_matches('\0');
            if value.len() > 64 || value.chars().any(char::is_control) {
                return None;
            }
            *target = value.into();
        }
        data = &data[end..];
    }
    (ad.manufacturer == "Howff" && !ad.owner.is_empty()).then_some(ad)
}

fn advert(room: &RoomEntry, bss: &str) -> Option<Advert> {
    let value = |name: &str| bss.lines().find_map(|line| line.strip_prefix(name));
    if value("ssid=")? != room.ssid || !value("bssid=")?.eq_ignore_ascii_case(&room.bssid) {
        return None;
    }
    ["ie=", "beacon_ie="].into_iter().find_map(|field| {
        let hex = value(field)?;
        if hex.len() > 16384 || hex.len() % 2 != 0 {
            return None;
        }
        let ie = crate::identity::decode(hex, hex.len() / 2).ok()?;
        parse_wps(&ie).filter(|ad| ad.room == room.token)
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LocalGame {
    pub path: PathBuf,
    pub name: String,
}

pub(crate) fn preview_game(cache: &Value, room: &RoomEntry, now: f64) -> Option<LocalGame> {
    let record = &cache["listings"][&room.id];
    if room.purpose() != crate::purpose::SessionPurpose::Game
        || now >= record["expires"].as_f64()?
    {
        return None;
    }
    LocalGame::deserialize(record.get("local_game")?).ok()
}

fn local_game<'a>(
    key: &str,
    index: &'a OnceLock<Option<Arc<Index>>>,
) -> Option<&'a crate::catalog::Entry> {
    let key = crate::identity::decode(key, 12).ok()?.try_into().ok()?;
    index
        .get_or_init(|| {
            let job = IndexJob::local().ok()?;
            Some(job.cached().unwrap_or_else(|| job.run()))
        })
        .as_ref()?
        .advertised_game(&key)
}

pub(crate) fn enrich(cache: &mut Value, rows: &mut Value, now: f64) {
    let Some(rows) = rows.as_array_mut() else {
        return;
    };
    let index = OnceLock::new();
    let mut next = serde_json::Map::new();
    for row in rows.iter_mut() {
        let Ok(room) = serde_json::from_value::<RoomEntry>(row.clone()) else {
            continue;
        };
        let saved = &cache["listings"][&room.id];
        let record = if saved["expires"].as_f64().is_some_and(|at| now < at) {
            saved.clone()
        } else {
            let ad = command("wpa_cli", &["-i", "wlan0", "bss", &room.bssid], 2, true)
                .ok()
                .and_then(|bss| advert(&room, &bss));
            let mut hint = None;
            let mut listing = Listing::default();
            let ttl = if let Some(ad) = ad {
                listing.owner = ad.owner;
                if room.purpose() == crate::purpose::SessionPurpose::Game {
                    listing.game = ad.game;
                    if let Some(entry) = local_game(&ad.key, &index) {
                        if listing.game.is_empty() {
                            listing.game = entry.name().into();
                        }
                        listing.artwork = entry
                            .image()
                            .map(|p| p.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        hint = Some(LocalGame {
                            path: entry.path().into(),
                            name: entry.name().into(),
                        });
                    }
                }
                60.0
            } else {
                listing.owner = crate::identity::remembered_name(&room.bssid).unwrap_or_default();
                2.0
            };
            json!({"expires":now+ttl,"listing":listing,"local_game":hint})
        };
        row["listing"] = record["listing"].clone();
        next.insert(room.id, record);
    }
    cache["listings"] = Value::Object(next);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn local_preview_requires_a_current_matching_game_room() {
        let room = RoomEntry::observed(
            "DIRECT-NP-012345-0123456789abc",
            "12:34:56:78:9a:bc",
            75,
            "",
        )
        .unwrap();
        let mut cache = json!({"listings":{room.id.clone():{
            "expires": 30.0,
            "local_game": {"path":"/roms/nes/game.nes","name":"Local title"}
        }}});
        assert_eq!(
            preview_game(&cache, &room, 29.0).unwrap().path,
            Path::new("/roms/nes/game.nes")
        );
        assert!(preview_game(&cache, &room, 30.0).is_none());
        let mut other = room.clone();
        other.id.push_str("-other");
        assert!(preview_game(&cache, &other, 29.0).is_none());
        other = room.clone();
        other.ssid = "DIRECT-NW-0123456789abc".into();
        assert!(preview_game(&cache, &other, 29.0).is_none());
        cache["listings"][&room.id]
            .as_object_mut()
            .unwrap()
            .remove("local_game");
        assert!(preview_game(&cache, &room, 29.0).is_none());
    }

    #[test]
    fn scan_metadata_is_bound_to_the_observed_group() {
        let ssid = "DIRECT-NP-abcdef-1111111111111";
        let mac = "02:00:00:00:00:01";
        let room = RoomEntry::observed(ssid, mac, 80, "").unwrap();
        let mut attrs = Vec::from([0, 0x50, 0xf2, 4]);
        for (tag, value) in [
            (0x1011u16, "阿梅的掌机"),
            (0x1021, "Howff"),
            (0x1023, "坦克大战"),
            (0x1042, &room.token),
        ] {
            attrs.extend(tag.to_be_bytes());
            attrs.extend((value.len() as u16).to_be_bytes());
            attrs.extend(value.as_bytes());
        }
        let mut ie = vec![221, attrs.len() as u8];
        ie.extend(attrs);
        let bss = format!(
            "bssid={mac}\nssid={ssid}\nie={}",
            crate::identity::encode(&ie)
        );
        let ad = advert(&room, &bss).unwrap();
        assert_eq!(
            (ad.owner.as_str(), ad.game.as_str()),
            ("阿梅的掌机", "坦克大战")
        );
        assert!(advert(&room, &bss.replace(mac, "02:00:00:00:00:02")).is_none());
        assert!(advert(&room, &bss.replace("31313131", "32323232")).is_none());
        assert!(advert(&room, &bss[..bss.len() - 2]).is_none());
        assert_eq!(
            portable_key(Path::new("/roms/snes/a.zip"), &["/roms".into()]),
            portable_key(Path::new("/roms2/snes/a.zip"), &["/roms2".into()])
        );
    }
}
