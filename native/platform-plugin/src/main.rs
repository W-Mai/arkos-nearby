use howff_platform::backend::PlatformSnapshot;
use howff_platform::ipc::{PluginRequest, PluginResponse, read_frame, write_frame};
use std::io::{self, BufReader, BufWriter};

const PLUGIN_ID: &str = "arkos-platform";
const TARGET: &str = "aarch64-unknown-linux-musl";

fn response(request: PluginRequest) -> PluginResponse {
    if let Err(error) = request.validate() {
        return PluginResponse {
            schema: howff_platform::ipc::FRAME_SCHEMA,
            request_id: request.request_id,
            ok: false,
            body: None,
            error: Some(error.to_string()),
        };
    }
    let snapshot = PlatformSnapshot::arkos(PLUGIN_ID, TARGET);
    match request.capability.as_str() {
        "paths" => PluginResponse {
            schema: howff_platform::ipc::FRAME_SCHEMA,
            request_id: request.request_id,
            ok: true,
            body: Some(serde_json::to_value(&snapshot).expect("snapshot serialization")),
            error: None,
        },
        "services" => PluginResponse {
            schema: howff_platform::ipc::FRAME_SCHEMA,
            request_id: request.request_id,
            ok: true,
            body: Some(serde_json::to_value(&snapshot.services).expect("service serialization")),
            error: None,
        },
        "recipes" => PluginResponse {
            schema: howff_platform::ipc::FRAME_SCHEMA,
            request_id: request.request_id,
            ok: true,
            body: Some(serde_json::to_value(&snapshot.recipes).expect("recipe serialization")),
            error: None,
        },
        "health" => PluginResponse {
            schema: howff_platform::ipc::FRAME_SCHEMA,
            request_id: request.request_id,
            ok: true,
            body: Some(serde_json::json!({"plugin_id": PLUGIN_ID, "target": TARGET})),
            error: None,
        },
        _ => PluginResponse {
            schema: howff_platform::ipc::FRAME_SCHEMA,
            request_id: request.request_id,
            ok: false,
            body: None,
            error: Some("Unsupported platform capability".into()),
        },
    }
}

fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = BufReader::new(stdin.lock());
    let mut output = BufWriter::new(stdout.lock());
    loop {
        let request = match read_frame::<_, PluginRequest>(&mut input) {
            Ok(request) => request,
            Err(howff_platform::ipc::Error::Io(error))
                if error.kind() == io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(error) => return Err(error.into()),
        };
        let reply = response(request);
        write_frame(&mut output, &reply)?;
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("platform plugin: {error}");
        std::process::exit(1);
    }
}
