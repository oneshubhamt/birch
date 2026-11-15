use super::{EditorCommand, serialize_editor_command};

pub const EDITOR_BRIDGE_GLOBAL: &str = "__rustnoteEditorBridge";

pub fn dispatch_expression(command: &EditorCommand) -> String {
    let envelope = serialize_editor_command(command)
        .unwrap_or_else(|_| r#"{"version":1,"command":"invalid","payload":{}}"#.to_string());
    format!("window.{EDITOR_BRIDGE_GLOBAL}?.dispatch({envelope})")
}

pub fn dispatch_script(command: &EditorCommand) -> String {
    format!("return {} ?? false;", dispatch_expression(command))
}

pub fn dispatch_string_script(command: &EditorCommand) -> String {
    format!("return {} ?? '';", dispatch_expression(command))
}

pub fn dispatch_usize_script(command: &EditorCommand) -> String {
    format!("return {} ?? 0;", dispatch_expression(command))
}

pub fn install_script(command: &EditorCommand, expected_marker: &str) -> String {
    let envelope = serialize_editor_command(command)
        .unwrap_or_else(|_| r#"{"version":1,"command":"invalid","payload":{}}"#.to_string());
    let marker = serde_json::to_string(expected_marker).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        r#"
const expectedMarker = {marker};
const deadline = Date.now() + 8000;
while (
  (!window.{EDITOR_BRIDGE_GLOBAL} || window.{EDITOR_BRIDGE_GLOBAL}.marker !== expectedMarker)
  && Date.now() < deadline
) {{
  await new Promise(resolve => window.setTimeout(resolve, 25));
}}
const bridge = window.{EDITOR_BRIDGE_GLOBAL};
if (!bridge) return false;
if (bridge.marker !== expectedMarker) return false;
return Boolean(bridge.dispatch({envelope}));
"#
    )
}

pub fn runtime_ready_script(generation: u64) -> String {
    format!(
        "await new Promise(resolve => window.setTimeout(resolve, 16)); return Boolean(window.{EDITOR_BRIDGE_GLOBAL}?.isReady({generation}));"
    )
}

pub fn runtime_error_script() -> String {
    format!(
        "const detail = window.{EDITOR_BRIDGE_GLOBAL}?.error?.() || (window.{EDITOR_BRIDGE_GLOBAL} ? 'The editor runtime returned no diagnostic.' : 'The local editor runtime asset did not load.'); return String(detail).split('\\n')[0].slice(0, 240);"
    )
}

pub fn wait_for_generation_then_dispatch_script(
    generation: u64,
    command: &EditorCommand,
) -> String {
    let envelope = serialize_editor_command(command)
        .unwrap_or_else(|_| r#"{"version":1,"command":"invalid","payload":{}}"#.to_string());
    format!(
        r#"
const expectedGeneration = {generation};
const deadline = Date.now() + 8000;
while (Date.now() < deadline) {{
  const bridge = window.{EDITOR_BRIDGE_GLOBAL};
  if (bridge?.isReady(expectedGeneration)) {{
    return bridge.dispatch({envelope});
  }}
  await new Promise(resolve => window.setTimeout(resolve, 25));
}}
return '';
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_targets_one_editor_bridge_namespace() {
        let script = dispatch_script(&EditorCommand::InsertLink {
            url: "https://example.test/\"quoted\"".to_string(),
            label: "x'); injected(); //".to_string(),
        });
        assert!(script.contains("window.__rustnoteEditorBridge?.dispatch("));
    }
}
