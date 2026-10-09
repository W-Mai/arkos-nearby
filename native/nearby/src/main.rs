use std::env;
use std::path::Path;

use arkos_nearby::{Result, require};
use arkos_nearby::{context, menu, session};
use serde_json::{Value, json};

fn run(args: &[String]) -> Result<i32> {
    match args.first().map(String::as_str) {
        Some("--version") => {
            println!(
                "arkos-nearby {} (native room runtime)",
                env!("CARGO_PKG_VERSION")
            );
            Ok(0)
        }
        Some("--help") | None => {
            println!(
                "arkos-nearby install [--interactive]\narkos-nearby uninstall\narkos-nearby update RELEASE_PATH\narkos-nearby doctor\narkos-nearby release-info\narkos-nearby identity show|reset|name NAME\narkos-nearby friends [forget DEVICE_ID]\narkos-nearby menu --frontend <retroarch|retroarch32> -- <launch args>\narkos-nearby action <name> <context.json> <payload.json>\narkos-nearby status\narkos-nearby manual\nWorker modes: worker, serve, game, game-recover, recover <session-id>"
            );
            Ok(0)
        }
        Some("status") => {
            session::identity(false)?;
            println!("{}", session::status(None)?);
            Ok(0)
        }
        Some("install") if args.len() == 1 || args.len() == 2 && args[1] == "--interactive" => {
            let result = arkos_nearby::installer::install()?;
            if args.len() == 2 {
                arkos_nearby::manual::install_result(&result)?;
            }
            println!("{result}");
            Ok(0)
        }
        Some("uninstall") if args.len() == 1 => {
            println!("{}", arkos_nearby::installer::uninstall()?);
            Ok(0)
        }
        Some("release-info") if args.len() == 1 => {
            println!("{}", arkos_nearby::installer::release_info()?);
            Ok(0)
        }
        Some("update") if args.len() == 2 => {
            println!("{}", arkos_nearby::installer::update(Path::new(&args[1]))?);
            Ok(0)
        }
        Some("doctor") => {
            let facts = arkos_nearby::compat::detect();
            println!(
                "{}",
                json!({"supported":facts.problems().is_empty(),"problems":facts.problems(),"facts":facts})
            );
            Ok(if facts.problems().is_empty() { 0 } else { 2 })
        }
        Some("manual") => arkos_nearby::manual::run_manual(),
        Some("identity") => {
            let value = match args.get(1).map(String::as_str) {
                Some("show") => {
                    session::identity(false)?;
                    json!(arkos_nearby::identity::load()?.public)
                }
                Some("reset") => {
                    arkos_nearby::identity::reset()?;
                    json!(arkos_nearby::identity::load()?.public)
                }
                Some("name") if args.len() == 3 => {
                    arkos_nearby::identity::rename(&args[2])?;
                    json!(arkos_nearby::identity::load()?.public)
                }
                _ => return Err("Usage: arkos-nearby identity show|reset|name NAME".into()),
            };
            println!("{value}");
            Ok(0)
        }
        Some("friends") => {
            session::identity(false)?;
            if args.get(1).is_some_and(|value| value == "forget") && args.len() == 3 {
                arkos_nearby::identity::forget(&args[2])?;
            } else {
                require(
                    args.len() == 1,
                    "Usage: arkos-nearby friends [forget DEVICE_ID]",
                )?;
            }
            println!("{}", arkos_nearby::identity::summary()?);
            Ok(0)
        }
        Some("pair") => {
            let value = match args.get(1).map(String::as_str) {
                Some("show") if args.len() == 2 => arkos_nearby::pairing::show()?,
                Some("new") if args.len() == 2 => arkos_nearby::pairing::new_code()?,
                Some("use") if args.len() == 3 => arkos_nearby::pairing::set(&args[2])?,
                _ => return Err("Usage: arkos-nearby pair show|new|use CODE".into()),
            };
            println!("{value}");
            Ok(0)
        }
        Some("inspect") => {
            session::identity(false)?;
            require(
                args.len() >= 3,
                "Choose an installed frontend and launch arguments",
            )?;
            let context = context::parse(&args[1], &args[2..])?;
            println!("{}", serde_json::to_string(&context)?);
            Ok(0)
        }
        Some("menu") => {
            session::identity(false)?;
            let mut offset = 1;
            let frontend = if args.get(offset).is_some_and(|v| v == "--frontend") {
                offset += 2;
                args.get(offset - 1).ok_or("Missing frontend")?.clone()
            } else if let Some(value) = args.get(offset).and_then(|v| v.strip_prefix("--frontend="))
            {
                offset += 1;
                value.into()
            } else {
                return Err("Missing local frontend".into());
            };
            if args.get(offset).is_some_and(|v| v == "--") {
                offset += 1;
            }
            menu::log(
                json!({"phase":"context","started_at":arkos_nearby::system::wall_time(),"frontend":frontend,"argv":&args[offset..]}),
            )?;
            let context = match context::parse(&frontend, &args[offset..]) {
                Ok(context) => {
                    menu::log(json!({"phase":"context_verified"}))?;
                    Some(context)
                }
                Err(error) => {
                    menu::log(json!({"phase":"context_failed","context_error":error.to_string()}))?;
                    None
                }
            };
            let backend = context
                .as_ref()
                .map(|context| session::NativeBackend::shared(context.clone()));
            menu::run(context, backend)
        }
        Some("action") => {
            require(
                args.len() == 4,
                "Action requires a local context file and payload",
            )?;
            session::identity(false)?;
            let context: context::Context = arkos_nearby::system::read(Path::new(&args[2]))?;
            let payload: Value = serde_json::from_str(&args[3])?;
            println!("{}", session::action(&args[1], &context, &payload)?);
            Ok(0)
        }
        Some("worker" | "serve" | "game" | "game-recover" | "recover") => {
            require(args.len() == 2, "Worker requires its owned session")?;
            match args[0].as_str() {
                "worker" => session::worker(&args[1])?,
                "serve" => {
                    session::identity(false)?;
                    arkos_nearby::http::serve(&args[1])?;
                }
                "game" => arkos_nearby::game::run(&args[1])?,
                "game-recover" => {
                    session::identity(false)?;
                    arkos_nearby::game::detached_recover(&args[1])?;
                }
                _ => {
                    println!(
                        "{}",
                        session::recover(
                            &args[1],
                            &env::var("ARKOS_NEARBY_RECOVERY").unwrap_or_else(|_| "exit".into())
                        )?
                    );
                }
            };
            Ok(0)
        }
        _ => Err("Unknown fixed runtime command".into()),
    }
}
fn main() {
    let dhcp_callback =
        env::var_os("ARKOS_NEARBY_DHCP_SESSION").is_some() && env::var_os("reason").is_some();
    if dhcp_callback
        || env::args_os().next().is_some_and(|path| {
            Path::new(&path).file_name() == Some(std::ffi::OsStr::new("dhcp-hook"))
        })
    {
        let result = arkos_nearby::radio::dhcp_hook();
        if let Err(error) = &result {
            eprintln!("DHCP lease callback: {error}");
        }
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }
    let args: Vec<_> = env::args().skip(1).collect();
    let code = match run(&args) {
        Ok(code) => code,
        Err(error) => {
            let _ = menu::log(json!({"phase":"failed","error":error.to_string()}));
            eprintln!("{}", json!({"error":error.to_string()}));
            1
        }
    };
    std::process::exit(code);
}
