//! Starting templates. Built programmatically because they depend on the
//! selected device (e.g. which pin carries the LED).

use crate::project_store::{
    Graph, GraphEdge, GraphNode, PinAssignment, PinMode, Position, Project, Template, VarType, Variable,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn node(id: &str, t: &str, x: f64, y: f64, props: Value) -> GraphNode {
    let properties: BTreeMap<String, Value> = props
        .as_object()
        .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();
    GraphNode {
        id: id.into(),
        block_type: t.into(),
        position: Position { x, y },
        properties,
    }
}

fn edge(src: &str, sh: &str, tgt: &str, th: &str) -> GraphEdge {
    GraphEdge {
        id: format!("e-{src}-{sh}-{tgt}-{th}"),
        source: src.into(),
        source_handle: sh.into(),
        target: tgt.into(),
        target_handle: th.into(),
    }
}

pub fn apply_template(project: &mut Project, template: Template, led_pin: &str) {
    let mut g = Graph::empty();
    let start = node("start", "program_start", 40.0, 120.0, json!({}));
    match template {
        Template::Empty => g.nodes.push(start),
        Template::Blink => {
            project.pins.assignments.push(PinAssignment {
                pin: led_pin.into(),
                label: "LED".into(),
                mode: PinMode::Output,
                initial_high: false,
            });
            g.nodes = vec![
                start,
                node("toggle", "pin_toggle", 320.0, 180.0, json!({ "pin": led_pin })),
                node("wait", "delay_ms", 580.0, 180.0, json!({ "ms": 500 })),
            ];
            g.edges = vec![
                edge("start", "loop", "toggle", "in"),
                edge("toggle", "next", "wait", "in"),
            ];
        }
        Template::UartEcho => {
            project.peripherals.usart.enabled = true;
            project.peripherals.usart.baud = 9600;
            g.variables.push(Variable {
                name: "rx".into(),
                var_type: VarType::Uint8,
                initial: 0,
            });
            g.nodes = vec![
                start,
                node(
                    "hello",
                    "uart_send_string",
                    320.0,
                    40.0,
                    json!({ "text": "UART echo ready", "newline": true }),
                ),
                node(
                    "recv",
                    "uart_receive_byte",
                    320.0,
                    220.0,
                    json!({ "variable": "rx" }),
                ),
                node("get", "var_get", 320.0, 380.0, json!({ "variable": "rx" })),
                node("send", "uart_send_byte", 600.0, 220.0, json!({})),
            ];
            g.edges = vec![
                edge("start", "setup", "hello", "in"),
                edge("start", "loop", "recv", "in"),
                edge("recv", "next", "send", "in"),
                edge("get", "value", "send", "value"),
            ];
        }
        Template::AdcReading => {
            project.peripherals.usart.enabled = true;
            project.peripherals.adc.enabled = true;
            g.nodes = vec![
                start,
                node("adc", "adc_read", 320.0, 340.0, json!({ "channel": 0 })),
                node(
                    "send",
                    "uart_send_number",
                    600.0,
                    180.0,
                    json!({ "newline": true }),
                ),
                node("wait", "delay_ms", 860.0, 180.0, json!({ "ms": 500 })),
            ];
            g.edges = vec![
                edge("start", "loop", "send", "in"),
                edge("adc", "value", "send", "value"),
                edge("send", "next", "wait", "in"),
            ];
        }
    }
    project.graph = g;
}
