use std::env;
use std::path::Path;

use howff::{Result, require};
use howff::{context, menu, session};
use serde_json::{Value, json};

fn run(args: &[String]) -> Result<i32> {
    match args.first().map(String::as_str) {
        Some("--version") => {
            println!("howff {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        Some("--help") | None => {
            println!(
                "howff install [--interactive]\nhowff uninstall\nhowff update RELEASE_PATH\nhowff doctor\nhowff release-info\nhowff identity show|reset|name NAME\nhowff friends [forget DEVICE_ID]\nhowff menu --frontend <retroarch|retroarch32> -- <launch args>\nhowff action <name> <context.json> <payload.json>\nhowff status\nhowff network <status|scan|create|join|approve|detach|cancel> [JSON]\nhowff manual\nhowff launcher\nWorker modes: worker, serve, game, game-recover, recover <session-id>"
            );
            Ok(0)
        }
        Some("status") => {
            session::identity(false)?;
            println!("{}", session::status(None)?);
            Ok(0)
        }
        Some("install") if args.len() == 1 || args.len() == 2 && args[1] == "--interactive" => {
            let result = howff::installer::install()?;
            if args.len() == 2 {
                howff::manual::install_result(&result)?;
            }
            println!("{result}");
            Ok(0)
        }
        Some("update-install") if args.len() == 2 => {
            println!("{}", howff::installer::install_for_update(&args[1])?);
            Ok(0)
        }
        Some("launcher-continue") if args.len() == 2 => {
            howff::updates::consume(&args[1])?;
            howff::home::continued()?;
            Ok(0)
        }
        Some("uninstall") if args.len() == 1 => {
            println!("{}", howff::installer::uninstall()?);
            Ok(0)
        }
        Some("release-info") if args.len() == 1 => {
            println!("{}", howff::installer::release_info()?);
            Ok(0)
        }
        Some("update") if args.len() == 2 => {
            println!("{}", howff::installer::update(Path::new(&args[1]))?);
            Ok(0)
        }
        Some("doctor") => {
            let facts = howff::compat::detect();
            println!(
                "{}",
                json!({"supported":facts.problems().is_empty(),"problems":facts.problems(),"facts":facts})
            );
            Ok(if facts.problems().is_empty() { 0 } else { 2 })
        }
        Some("manual") => howff::manual::run_manual(),
        Some("launcher") if args.len() == 1 => {
            howff::home::launch()?;
            Ok(0)
        }
        Some("launcher") if args.len() == 2 && args[1] == "--standalone" => {
            howff::home::launch_standalone()?;
            Ok(0)
        }
        Some("launcher-standalone") if args.len() == 1 => {
            howff::home::standalone()?;
            Ok(0)
        }
        Some("home-dispatch") if args.len() == 1 => {
            howff::home::dispatch()?;
            Ok(0)
        }
        Some("launcher-game-end") if args.len() == 1 => {
            howff::launcher::recipes::finish()?;
            Ok(0)
        }
        Some("resolve-launch") if args.len() >= 4 && args[1] == "--frontend" && args[3] == "--" => {
            howff::launch::resolve(&args[2], &args[4..])?;
            Ok(0)
        }
        Some("launch-end") if args.len() == 2 => {
            howff::launch::finish(&args[1])?;
            Ok(0)
        }
        Some("launcher-open") if args.len() == 2 => {
            howff::home::open_launcher(args[1].parse()?)?;
            Ok(0)
        }
        Some("home-listen" | "home-open") if args.len() == 2 => {
            let pid = args[1].parse::<u32>()?;
            if args[0] == "home-listen" {
                howff::home::listen(pid)?;
            } else {
                howff::home::open(pid)?;
            }
            Ok(0)
        }
        Some("home-resume") if args.len() == 1 => {
            howff::wifi::recover_pending()?;
            howff::home::resume()?;
            Ok(0)
        }
        Some("wifi-recover") if args.len() == 2 => {
            howff::wifi::recover(&args[1])?;
            Ok(0)
        }
        Some("network") if (2..=3).contains(&args.len()) => {
            let payload = args
                .get(2)
                .map(|value| serde_json::from_str::<Value>(value))
                .transpose()?
                .unwrap_or_else(|| json!({}));
            println!(
                "{}",
                session::action_for(
                    &args[1],
                    &howff::purpose::SessionIntent::NetworkOnly,
                    &payload
                )?
            );
            Ok(0)
        }
        Some("identity") => {
            let value = match args.get(1).map(String::as_str) {
                Some("show") => {
                    session::identity(false)?;
                    json!(howff::identity::load()?.public)
                }
                Some("reset") => {
                    howff::identity::reset()?;
                    json!(howff::identity::load()?.public)
                }
                Some("name") if args.len() == 3 => {
                    howff::identity::rename(&args[2])?;
                    json!(howff::identity::load()?.public)
                }
                _ => return Err("Usage: howff identity show|reset|name NAME".into()),
            };
            println!("{value}");
            Ok(0)
        }
        Some("friends") => {
            session::identity(false)?;
            if args.get(1).is_some_and(|value| value == "forget") && args.len() == 3 {
                howff::identity::forget(&args[2])?;
            } else {
                require(args.len() == 1, "Usage: howff friends [forget DEVICE_ID]")?;
            }
            println!("{}", howff::identity::summary()?);
            Ok(0)
        }
        Some("pair") => {
            let value = match args.get(1).map(String::as_str) {
                Some("show") if args.len() == 2 => howff::pairing::show()?,
                Some("new") if args.len() == 2 => howff::pairing::new_code()?,
                Some("use") if args.len() == 3 => howff::pairing::set(&args[2])?,
                _ => return Err("Usage: howff pair show|new|use CODE".into()),
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
            if session::state().is_ok_and(|state| {
                state.purpose == howff::purpose::SessionPurpose::NetworkOnly
                    && (!state.closed() || !state.network_restored)
            }) && menu::run_network()?
            {
                return Ok(0);
            }
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
                json!({"phase":"context","started_at":howff::system::wall_time(),"frontend":frontend,"argv":&args[offset..]}),
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
            let context: context::Context = howff::system::read(Path::new(&args[2]))?;
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
                    howff::http::serve(&args[1])?;
                }
                "game" => howff::game::run(&args[1])?,
                "game-recover" => {
                    session::identity(false)?;
                    howff::game::detached_recover(&args[1])?;
                }
                _ => {
                    println!(
                        "{}",
                        session::recover(
                            &args[1],
                            &env::var("HOWFF_RECOVERY").unwrap_or_else(|_| "exit".into())
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
        env::var_os("HOWFF_DHCP_SESSION").is_some() && env::var_os("reason").is_some();
    if dhcp_callback
        || env::args_os().next().is_some_and(|path| {
            Path::new(&path).file_name() == Some(std::ffi::OsStr::new("dhcp-hook"))
        })
    {
        let result = howff::radio::dhcp_hook();
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
