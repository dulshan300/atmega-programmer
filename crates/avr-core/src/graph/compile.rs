//! Graph validation and lowering to IR.
//!
//! Execution semantics:
//! * Exec ports form singly-linked chains. Each exec output connects to at
//!   most one block and each exec input has at most one incoming edge, so the
//!   execution order is never ambiguous. Branches re-join through the `next`
//!   output of the branching block.
//! * Data inputs have at most one incoming edge; data outputs may fan out.
//!   Expression blocks are evaluated where they are used.
//! * Cycles in exec chains or data expressions are rejected.

use super::catalog::{BlockCatalog, BlockDef, PortType, PropertyDef, PropertyKind};
use super::ir::*;
use crate::device_registry::{DeviceDefinition, RegisterProfile};
use crate::diagnostics::{Diagnostic, has_errors, sort_diagnostics};
use crate::paths::is_valid_identifier;
use crate::project_store::{GraphNode, PinMode, Project, VarType};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashSet};

type PortKey = (String, String);

struct Ctx<'a> {
    catalog: &'a BlockCatalog,
    device: &'a DeviceDefinition,
    project: &'a Project,
    nodes: BTreeMap<&'a str, (&'a GraphNode, &'a BlockDef)>,
    variables: BTreeMap<&'a str, VarType>,
    exec_out: BTreeMap<PortKey, Vec<String>>,
    data_in: BTreeMap<PortKey, Vec<PortKey>>,
    diags: Vec<Diagnostic>,
    visited_stmts: HashSet<String>,
    exec_stack: Vec<String>,
    expr_stack: Vec<String>,
    uses: Uses,
}

/// Validates the project's visual graph and lowers it to IR.
/// Returns `None` for the IR when any error diagnostic was produced.
pub fn compile_graph(
    catalog: &BlockCatalog,
    device: &DeviceDefinition,
    project: &Project,
) -> (Option<IrProgram>, Vec<Diagnostic>) {
    let mut ctx = Ctx {
        catalog,
        device,
        project,
        nodes: BTreeMap::new(),
        variables: BTreeMap::new(),
        exec_out: BTreeMap::new(),
        data_in: BTreeMap::new(),
        diags: vec![],
        visited_stmts: HashSet::new(),
        exec_stack: vec![],
        expr_stack: vec![],
        uses: Uses::default(),
    };
    ctx.index_nodes();
    ctx.check_variables();
    ctx.index_edges();
    ctx.check_properties();
    let program = ctx.lower();
    ctx.report_unreachable();
    let mut diags = ctx.diags;
    sort_diagnostics(&mut diags);
    if has_errors(&diags) {
        (None, diags)
    } else {
        (program, diags)
    }
}

impl<'a> Ctx<'a> {
    fn err(&mut self, node: &str, code: &str, msg: impl Into<String>) {
        self.diags.push(Diagnostic::error(code, msg).at_node(node));
    }
    fn warn(&mut self, node: &str, code: &str, msg: impl Into<String>) {
        self.diags.push(Diagnostic::warning(code, msg).at_node(node));
    }

    fn index_nodes(&mut self) {
        let mut unique_seen: BTreeMap<&str, &str> = BTreeMap::new();
        for node in &self.project.graph.nodes {
            if self.nodes.contains_key(node.id.as_str()) {
                self.err(
                    &node.id,
                    "graph.duplicate_node",
                    format!("Duplicate node id '{}'", node.id),
                );
                continue;
            }
            let Some(def) = self.catalog.get(&node.block_type) else {
                self.err(
                    &node.id,
                    "graph.unknown_block",
                    format!("Unknown block type '{}'", node.block_type),
                );
                continue;
            };
            if def.unique {
                if let Some(first) = unique_seen.get(def.block_type.as_str()) {
                    let msg = format!(
                        "Only one '{}' block is allowed (also used by node {first})",
                        def.label
                    );
                    self.err(&node.id, "graph.duplicate_unique", msg);
                } else {
                    unique_seen.insert(&def.block_type, &node.id);
                }
            }
            for key in node.properties.keys() {
                if def.property(key).is_none() {
                    self.warn(
                        &node.id,
                        "graph.unknown_property",
                        format!("Ignoring unknown property '{key}'"),
                    );
                }
            }
            self.nodes.insert(&node.id, (node, def));
        }
        if !unique_seen.contains_key("program_start") {
            self.diags.push(Diagnostic::error(
                "graph.no_start",
                "The program needs exactly one 'Program Start' block",
            ));
        }
    }

    fn check_variables(&mut self) {
        for v in &self.project.graph.variables {
            if !is_valid_identifier(&v.name) {
                self.diags.push(
                    Diagnostic::error(
                        "graph.variable_name",
                        format!("Variable name '{}' is not a valid C identifier", v.name),
                    )
                    .in_section("variables"),
                );
                continue;
            }
            if self.variables.insert(&v.name, v.var_type).is_some() {
                self.diags.push(
                    Diagnostic::error(
                        "graph.variable_duplicate",
                        format!("Variable '{}' is declared twice", v.name),
                    )
                    .in_section("variables"),
                );
            }
            let (lo, hi) = v.var_type.range();
            if v.initial < lo || v.initial > hi {
                self.diags.push(
                    Diagnostic::error(
                        "graph.variable_initial",
                        format!(
                            "Initial value {} of '{}' is outside {}..={}",
                            v.initial, v.name, lo, hi
                        ),
                    )
                    .in_section("variables"),
                );
            }
        }
    }

    fn resolved_port_type(&self, node: &GraphNode, t: PortType) -> Option<PortType> {
        if t != PortType::Variable {
            return Some(t);
        }
        let name = node.properties.get("variable").and_then(Value::as_str)?;
        self.variables.get(name).map(|vt| {
            if *vt == VarType::Bool {
                PortType::Bool
            } else {
                PortType::Int
            }
        })
    }

    fn index_edges(&mut self) {
        let mut seen_ids = BTreeSet::new();
        for e in &self.project.graph.edges {
            if !seen_ids.insert(&e.id) {
                self.diags.push(Diagnostic::error(
                    "graph.duplicate_edge",
                    format!("Duplicate edge id '{}'", e.id),
                ));
                continue;
            }
            let (Some((sn, sd)), Some((tn, td))) = (
                self.nodes.get(e.source.as_str()).copied(),
                self.nodes.get(e.target.as_str()).copied(),
            ) else {
                self.diags.push(Diagnostic::error(
                    "graph.dangling_edge",
                    format!("Connection '{}' refers to a missing block", e.id),
                ));
                continue;
            };
            let (Some(sp), Some(tp)) = (sd.output(&e.source_handle), td.input(&e.target_handle)) else {
                self.err(
                    &e.target,
                    "graph.bad_port",
                    format!("Connection '{}' uses a port that does not exist", e.id),
                );
                continue;
            };
            if e.source == e.target {
                self.err(
                    &e.target,
                    "graph.self_loop",
                    format!("'{}' cannot connect to itself", td.label),
                );
                continue;
            }
            let (Some(st), Some(tt)) = (
                self.resolved_port_type(sn, sp.port_type),
                self.resolved_port_type(tn, tp.port_type),
            ) else {
                // A variable port without a selected variable; reported by property checks.
                continue;
            };
            if st != tt {
                self.err(
                    &e.target,
                    "graph.type_mismatch",
                    format!(
                        "Cannot connect {} output '{}' of '{}' to {} input '{}' of '{}'",
                        type_name(st),
                        sp.label,
                        sd.label,
                        type_name(tt),
                        tp.label,
                        td.label
                    ),
                );
                continue;
            }
            if st == PortType::Exec {
                self.exec_out
                    .entry((e.source.clone(), e.source_handle.clone()))
                    .or_default()
                    .push(e.target.clone());
            } else {
                self.data_in
                    .entry((e.target.clone(), e.target_handle.clone()))
                    .or_default()
                    .push((e.source.clone(), e.source_handle.clone()));
            }
        }
        let mut exec_in_count: BTreeMap<String, usize> = BTreeMap::new();
        let mut problems = vec![];
        for ((node, port), targets) in &self.exec_out {
            if targets.len() > 1 {
                problems.push((
                    node.clone(),
                    "graph.exec_fanout",
                    format!("Output '{port}' connects to {} blocks; execution order would be ambiguous. Use a single chain.", targets.len()),
                ));
            }
            for t in targets {
                *exec_in_count.entry(t.clone()).or_default() += 1;
            }
        }
        for (node, count) in exec_in_count {
            if count > 1 {
                problems.push((
                    node,
                    "graph.exec_merge",
                    format!("This block is reached from {count} places. Continue from the 'next' output of the branching block instead."),
                ));
            }
        }
        for ((node, port), sources) in &self.data_in {
            if sources.len() > 1 {
                problems.push((
                    node.clone(),
                    "graph.data_multi",
                    format!("Input '{port}' has more than one connection"),
                ));
            }
        }
        for (n, c, m) in problems {
            self.err(&n, c, m);
        }
    }

    fn prop(&self, node: &GraphNode, def: &PropertyDef) -> Value {
        node.properties
            .get(&def.id)
            .cloned()
            .unwrap_or_else(|| def.default.clone())
    }

    fn check_properties(&mut self) {
        let nodes: Vec<_> = self.nodes.values().copied().collect();
        for (node, def) in nodes {
            for p in &def.properties {
                let v = self.prop(node, p);
                if let Err(msg) = self.check_property(p, &v) {
                    self.err(&node.id, "graph.property", format!("{}: {msg}", p.label));
                }
            }
        }
    }

    fn check_property(&self, p: &PropertyDef, v: &Value) -> Result<(), String> {
        match p.kind {
            PropertyKind::Int => {
                let n = v.as_i64().ok_or("must be an integer")?;
                if p.min.is_some_and(|m| n < m) || p.max.is_some_and(|m| n > m) {
                    return Err(format!(
                        "must be between {} and {}",
                        p.min.unwrap_or(i64::MIN),
                        p.max.unwrap_or(i64::MAX)
                    ));
                }
            }
            PropertyKind::Bool => {
                v.as_bool().ok_or("must be true or false")?;
            }
            PropertyKind::String => {
                let s = v.as_str().ok_or("must be text")?;
                if p.max_length.is_some_and(|m| s.chars().count() > m) {
                    return Err(format!(
                        "must be at most {} characters",
                        p.max_length.unwrap_or(0)
                    ));
                }
            }
            PropertyKind::Enum => {
                let s = v.as_str().ok_or("must be one of the listed options")?;
                if !p.options.iter().any(|o| o.value == s) {
                    return Err(format!("'{s}' is not a valid option"));
                }
            }
            PropertyKind::Pin => {
                let s = v.as_str().unwrap_or("");
                if s.is_empty() {
                    return Err("select a pin".into());
                }
                if self.device.pin(s).is_none() {
                    return Err(format!("{s} is not a GPIO pin on {}", self.device.name));
                }
                if !self
                    .device
                    .pin_in_package(&self.project.manifest.device.package, s)
                {
                    return Err(format!(
                        "{s} is not bonded out in package {}",
                        self.project.manifest.device.package
                    ));
                }
                if self.assignment(s).is_none() {
                    return Err(format!("{s} is not assigned in Pin Configuration"));
                }
            }
            PropertyKind::Variable => {
                let s = v.as_str().unwrap_or("");
                if s.is_empty() {
                    if !p.optional {
                        return Err("select a variable".into());
                    }
                } else if !self.variables.contains_key(s) {
                    return Err(format!("variable '{s}' does not exist"));
                }
            }
            PropertyKind::Identifier => {
                let s = v.as_str().unwrap_or("");
                if !is_valid_identifier(s) {
                    return Err(format!("'{s}' is not a valid C identifier"));
                }
            }
            PropertyKind::AdcChannel => {
                let n = v.as_u64().ok_or("must be a channel number")?;
                let pkg = &self.project.manifest.device.package;
                if n > 255 || self.device.adc_channel(pkg, n as u8).is_none() {
                    return Err(format!(
                        "ADC channel {n} is not available on {} {pkg}",
                        self.device.name
                    ));
                }
            }
            PropertyKind::ExtInterrupt => {
                let s = v.as_str().unwrap_or("");
                if !self
                    .device
                    .peripherals
                    .external_interrupts
                    .iter()
                    .any(|e| e.id == s)
                {
                    return Err(format!("{s} does not exist on {}", self.device.name));
                }
            }
        }
        Ok(())
    }

    fn assignment(&self, pin: &str) -> Option<&'a crate::project_store::PinAssignment> {
        self.project.pins.assignments.iter().find(|a| a.pin == pin)
    }

    fn pin_ref(&self, node: &GraphNode) -> Option<PinRef> {
        let name = node.properties.get("pin").and_then(Value::as_str)?;
        let def = self.device.pin(name)?;
        let a = self.assignment(name)?;
        Some(PinRef {
            name: name.to_string(),
            label: a.label.clone(),
            port: def.port.clone(),
            bit: def.bit,
        })
    }

    fn p_str(&self, node: &GraphNode, def: &BlockDef, id: &str) -> String {
        def.property(id)
            .map(|p| self.prop(node, p))
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    }
    fn p_i64(&self, node: &GraphNode, def: &BlockDef, id: &str) -> i64 {
        def.property(id)
            .map(|p| self.prop(node, p))
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
    }
    fn p_bool(&self, node: &GraphNode, def: &BlockDef, id: &str) -> bool {
        def.property(id)
            .map(|p| self.prop(node, p))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    fn lower(&mut self) -> Option<IrProgram> {
        let start = self
            .nodes
            .values()
            .find(|(_, d)| d.block_type == "program_start")
            .map(|(n, _)| n.id.clone());
        let setup = start.as_ref().map(|s| self.chain(s, "setup")).unwrap_or_default();
        let loop_body = start.as_ref().map(|s| self.chain(s, "loop")).unwrap_or_default();

        let mut functions = vec![];
        let mut function_names = BTreeMap::new();
        let defs: Vec<_> = self
            .nodes
            .values()
            .filter(|(_, d)| d.block_type == "function_def")
            .map(|(n, d)| (*n, *d))
            .collect();
        for (node, def) in defs {
            let name = self.p_str(node, def, "name");
            if let Some(first) = function_names.insert(name.clone(), node.id.clone()) {
                self.err(
                    &node.id,
                    "graph.function_duplicate",
                    format!("Function '{name}' is also defined by node {first}"),
                );
            }
            let body = self.chain(&node.id, "body");
            functions.push(IrFunction {
                name,
                node_id: node.id.clone(),
                body,
            });
        }
        let calls: Vec<_> = self
            .nodes
            .values()
            .filter(|(_, d)| d.block_type == "function_call")
            .map(|(n, d)| (n.id.clone(), self.p_str(n, d, "name")))
            .collect();
        for (id, name) in calls {
            if !function_names.contains_key(&name) {
                self.err(
                    &id,
                    "graph.function_missing",
                    format!("No Function Definition named '{name}'"),
                );
            }
        }

        let mut external_interrupts = vec![];
        let mut sources = BTreeMap::new();
        let isrs: Vec<_> = self
            .nodes
            .values()
            .filter(|(_, d)| d.block_type == "external_interrupt")
            .map(|(n, d)| (*n, *d))
            .collect();
        for (node, def) in isrs {
            let source = self.p_str(node, def, "source");
            let trigger = match self.p_str(node, def, "trigger").as_str() {
                "low" => ExtTrigger::Low,
                "change" => ExtTrigger::Change,
                "rising" => ExtTrigger::Rising,
                _ => ExtTrigger::Falling,
            };
            if let Some(first) = sources.insert(source.clone(), node.id.clone()) {
                self.err(
                    &node.id,
                    "graph.isr_duplicate",
                    format!("{source} already has a handler (node {first})"),
                );
            }
            if source == "INT2"
                && self.device.register_profile == RegisterProfile::MegaLegacy
                && matches!(trigger, ExtTrigger::Low | ExtTrigger::Change)
            {
                self.err(
                    &node.id,
                    "graph.int2_trigger",
                    "INT2 only supports rising or falling edges on this device",
                );
            }
            if let Some(ext) = self
                .device
                .peripherals
                .external_interrupts
                .iter()
                .find(|e| e.id == source)
                && self
                    .assignment(&ext.pin)
                    .is_some_and(|a| a.mode == PinMode::Output)
            {
                let msg = format!("{source} uses pin {}, which is configured as an output", ext.pin);
                self.warn(&node.id, "graph.isr_pin_output", msg);
            }
            let body = self.chain(&node.id, "body");
            external_interrupts.push(IrExternalInterrupt {
                vector: format!("{source}_vect"),
                source,
                trigger,
                node_id: node.id.clone(),
                body,
            });
        }

        if self.uses.uart && !self.project.peripherals.usart.enabled {
            self.diags.push(
                Diagnostic::error(
                    "graph.uart_disabled",
                    "UART blocks are used but the USART is disabled in Peripherals",
                )
                .in_section("peripherals"),
            );
        }
        if self.uses.adc && !self.project.peripherals.adc.enabled {
            self.diags.push(
                Diagnostic::error(
                    "graph.adc_disabled",
                    "ADC blocks are used but the ADC is disabled in Peripherals",
                )
                .in_section("peripherals"),
            );
        }

        let variables = self
            .project
            .graph
            .variables
            .iter()
            .map(|v| IrVariable {
                name: v.name.clone(),
                var_type: v.var_type,
                initial: v.initial,
            })
            .collect();
        Some(IrProgram {
            variables,
            functions,
            external_interrupts,
            setup,
            loop_body,
            uses: self.uses.clone(),
        })
    }

    /// Follows an exec chain starting at `node`'s output `port`.
    fn chain(&mut self, node: &str, port: &str) -> Vec<Stmt> {
        let mut out = vec![];
        let mut cur = self.next_of(node, port);
        while let Some(id) = cur {
            if self.exec_stack.contains(&id) {
                self.err(
                    &id,
                    "graph.exec_cycle",
                    "Execution cycle detected. Use a While or Repeat block for loops.",
                );
                break;
            }
            if !self.visited_stmts.insert(id.clone()) {
                // Already lowered elsewhere; reported as a merge error during edge indexing.
                break;
            }
            self.exec_stack.push(id.clone());
            let stmt = self.lower_stmt(&id);
            self.exec_stack.pop();
            let Some(stmt) = stmt else { break };
            let terminal = matches!(stmt.kind, StmtKind::Return);
            out.push(stmt);
            if terminal {
                break;
            }
            cur = self.next_of(&id, "next");
        }
        out
    }

    fn next_of(&self, node: &str, port: &str) -> Option<String> {
        self.exec_out
            .get(&(node.to_string(), port.to_string()))
            .and_then(|v| v.first().cloned())
    }

    fn lower_stmt(&mut self, id: &str) -> Option<Stmt> {
        let (node, def) = *self.nodes.get(id)?;
        if def.input("in").is_none_or(|p| p.port_type != PortType::Exec) {
            self.err(
                id,
                "graph.not_statement",
                format!("'{}' cannot be part of an execution chain", def.label),
            );
            return None;
        }
        let pin = || self.pin_ref(node);
        let kind = match def.block_type.as_str() {
            "if_else" => {
                let cond = self.expr(node, def, "cond")?;
                StmtKind::If {
                    cond,
                    then_body: self.chain(id, "then"),
                    else_body: self.chain(id, "else"),
                }
            }
            "while_loop" => {
                let cond = self.expr(node, def, "cond")?;
                StmtKind::While {
                    cond,
                    body: self.chain(id, "body"),
                }
            }
            "repeat" => {
                let count = self.expr(node, def, "count")?;
                let idx = self.p_str(node, def, "indexVariable");
                StmtKind::Repeat {
                    count,
                    index_var: if idx.is_empty() { None } else { Some(idx) },
                    body: self.chain(id, "body"),
                }
            }
            "function_call" => StmtKind::Call {
                function: self.p_str(node, def, "name"),
            },
            "return" => StmtKind::Return,
            "pin_mode" => {
                let mode = match self.p_str(node, def, "mode").as_str() {
                    "input" => PinMode::Input,
                    "input_pullup" => PinMode::InputPullup,
                    _ => PinMode::Output,
                };
                StmtKind::PinMode { pin: pin()?, mode }
            }
            "pin_write" | "pin_toggle" => {
                let p = pin()?;
                if self
                    .assignment(&p.name)
                    .is_some_and(|a| a.mode != PinMode::Output)
                {
                    self.warn(
                        id,
                        "graph.write_input_pin",
                        format!("{} is configured as an input; writing it changes the pull-up instead of driving the pin", p.name),
                    );
                }
                if def.block_type == "pin_toggle" {
                    StmtKind::PinToggle { pin: p }
                } else {
                    StmtKind::PinWrite {
                        value: self.expr(node, def, "value")?,
                        pin: p,
                    }
                }
            }
            "wait_pin" => StmtKind::WaitPin {
                pin: pin()?,
                level: self.p_bool(node, def, "level"),
            },
            "delay_ms" => {
                let ms = self.expr(node, def, "ms")?;
                if !matches!(ms, Expr::Int { .. }) {
                    self.uses.variable_delay = true;
                }
                StmtKind::DelayMs { ms }
            }
            "delay_us" => StmtKind::DelayUs {
                us: self.p_i64(node, def, "us") as u32,
            },
            "var_set" => StmtKind::SetVar {
                name: self.p_str(node, def, "variable"),
                value: self.expr(node, def, "value")?,
            },
            "uart_send_string" => {
                self.uses.uart = true;
                StmtKind::UartSendString {
                    text: self.p_str(node, def, "text"),
                    newline: self.p_bool(node, def, "newline"),
                }
            }
            "uart_send_number" => {
                self.uses.uart = true;
                self.uses.uart_number = true;
                StmtKind::UartSendNumber {
                    value: self.expr(node, def, "value")?,
                    newline: self.p_bool(node, def, "newline"),
                }
            }
            "uart_send_byte" => {
                self.uses.uart = true;
                StmtKind::UartSendByte {
                    value: self.expr(node, def, "value")?,
                }
            }
            "uart_receive_byte" => {
                self.uses.uart = true;
                StmtKind::UartReceiveByte {
                    var: self.p_str(node, def, "variable"),
                }
            }
            other => {
                self.err(
                    id,
                    "graph.unsupported",
                    format!("Block '{other}' is not supported in execution chains"),
                );
                return None;
            }
        };
        Some(Stmt {
            node_id: id.to_string(),
            label: def.label.clone(),
            kind,
        })
    }

    /// Lowers a data input: connected expression, else fallback property, else error.
    fn expr(&mut self, node: &GraphNode, def: &BlockDef, input: &str) -> Option<Expr> {
        let port = def.input(input)?;
        let source = self
            .data_in
            .get(&(node.id.clone(), input.to_string()))
            .and_then(|v| v.first().cloned());
        if let Some((src_node, _src_port)) = source {
            return self.expr_node(&src_node);
        }
        if let Some(fp) = &port.fallback_property {
            let pd = def.property(fp)?;
            let v = self.prop(node, pd);
            return match pd.kind {
                PropertyKind::Bool => Some(Expr::Bool {
                    value: v.as_bool().unwrap_or(false),
                }),
                _ => Some(Expr::Int {
                    value: v.as_i64().unwrap_or(0),
                }),
            };
        }
        let label = if port.label.is_empty() {
            &port.id
        } else {
            &port.label
        };
        self.err(
            &node.id,
            "graph.input_required",
            format!("Input '{label}' of '{}' must be connected", def.label),
        );
        None
    }

    fn expr_node(&mut self, id: &str) -> Option<Expr> {
        if self.expr_stack.iter().any(|s| s == id) {
            self.err(id, "graph.data_cycle", "Data connections form a cycle");
            return None;
        }
        let (node, def) = *self.nodes.get(id)?;
        self.expr_stack.push(id.to_string());
        let bin = |this: &mut Self, op: BinOp| -> Option<Expr> {
            let a = this.expr(node, def, "a")?;
            let b = this.expr(node, def, "b")?;
            Some(Expr::Binary {
                op,
                a: Box::new(a),
                b: Box::new(b),
            })
        };
        let e = match def.block_type.as_str() {
            "const_int" => Some(Expr::Int {
                value: self.p_i64(node, def, "value"),
            }),
            "const_bool" => Some(Expr::Bool {
                value: self.p_bool(node, def, "value"),
            }),
            "var_get" => Some(Expr::Var {
                name: self.p_str(node, def, "variable"),
            }),
            "pin_read" => self.pin_ref(node).map(|pin| Expr::PinRead { pin }),
            "adc_read" => {
                self.uses.adc = true;
                Some(Expr::AdcRead {
                    channel: self.p_i64(node, def, "channel") as u8,
                })
            }
            "adc_to_millivolts" => {
                let raw = self.expr(node, def, "raw");
                raw.map(|r| Expr::AdcToMillivolts {
                    raw: Box::new(r),
                    reference_mv: self.p_i64(node, def, "referenceMillivolts"),
                })
            }
            "uart_available" => {
                self.uses.uart = true;
                Some(Expr::UartAvailable)
            }
            "not" => self.expr(node, def, "a").map(|a| Expr::Not { a: Box::new(a) }),
            "math" | "bitwise" | "compare" | "logic" => {
                let op = match (def.block_type.as_str(), self.p_str(node, def, "op").as_str()) {
                    ("math", "sub") => BinOp::Sub,
                    ("math", "mul") => BinOp::Mul,
                    ("math", "div") => BinOp::Div,
                    ("math", "mod") => BinOp::Mod,
                    ("math", _) => BinOp::Add,
                    ("bitwise", "or") => BinOp::BitOr,
                    ("bitwise", "xor") => BinOp::BitXor,
                    ("bitwise", "shl") => BinOp::Shl,
                    ("bitwise", "shr") => BinOp::Shr,
                    ("bitwise", _) => BinOp::BitAnd,
                    ("compare", "ne") => BinOp::Ne,
                    ("compare", "lt") => BinOp::Lt,
                    ("compare", "le") => BinOp::Le,
                    ("compare", "gt") => BinOp::Gt,
                    ("compare", "ge") => BinOp::Ge,
                    ("compare", _) => BinOp::Eq,
                    ("logic", "or") => BinOp::Or,
                    _ => BinOp::And,
                };
                bin(self, op)
            }
            other => {
                self.err(
                    id,
                    "graph.not_expression",
                    format!("Block '{other}' does not produce a value"),
                );
                None
            }
        };
        self.expr_stack.pop();
        e
    }

    fn report_unreachable(&mut self) {
        let unreachable: Vec<_> = self
            .nodes
            .values()
            .filter(|(n, d)| {
                d.input("in").is_some_and(|p| p.port_type == PortType::Exec)
                    && !self.visited_stmts.contains(&n.id)
            })
            .map(|(n, d)| (n.id.clone(), d.label.clone()))
            .collect();
        for (id, label) in unreachable {
            self.warn(
                &id,
                "graph.unreachable",
                format!("'{label}' is not connected to any execution chain and will not run"),
            );
        }
    }
}

fn type_name(t: PortType) -> &'static str {
    match t {
        PortType::Exec => "execution",
        PortType::Bool => "boolean",
        PortType::Int => "integer",
        PortType::Variable => "variable",
    }
}
