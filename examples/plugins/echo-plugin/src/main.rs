use anyhow::Result;
use std::io::{self, Read};
use structural_plugin_sdk::{
    PluginError, PluginInvocation, PluginResponse, PluginStatus, PLUGIN_PROTOCOL_VERSION,
    PLUGIN_SCHEMA_VERSION,
};

fn main() -> Result<()> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let request: PluginInvocation = serde_json::from_str(&input)?;

    let response = if request.method == "echo" {
        PluginResponse {
            schema_version: PLUGIN_SCHEMA_VERSION.to_owned(),
            protocol_version: PLUGIN_PROTOCOL_VERSION.to_owned(),
            invocation_id: request.invocation_id,
            status: PluginStatus::Ok,
            result: Some(request.params),
            error: None,
        }
    } else {
        PluginResponse {
            schema_version: PLUGIN_SCHEMA_VERSION.to_owned(),
            protocol_version: PLUGIN_PROTOCOL_VERSION.to_owned(),
            invocation_id: request.invocation_id,
            status: PluginStatus::Error,
            result: None,
            error: Some(PluginError {
                code: "method_not_found".to_owned(),
                message: format!("unknown example method '{}'", request.method),
            }),
        }
    };
    println!("{}", serde_json::to_string(&response)?);
    Ok(())
}
