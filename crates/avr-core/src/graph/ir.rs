//! Typed intermediate representation produced from a validated graph.
//! The code generator consumes only this structure, never raw UI data.

use crate::project_store::{PinMode, VarType};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PinRef {
    pub name: String,
    pub label: String,
    pub port: String,
    pub bit: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinOp {
    pub fn c_op(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Expr {
    Int { value: i64 },
    Bool { value: bool },
    Var { name: String },
    PinRead { pin: PinRef },
    AdcRead { channel: u8 },
    AdcToMillivolts { raw: Box<Expr>, reference_mv: i64 },
    UartAvailable,
    Binary { op: BinOp, a: Box<Expr>, b: Box<Expr> },
    Not { a: Box<Expr> },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StmtKind {
    PinMode {
        pin: PinRef,
        mode: PinMode,
    },
    PinWrite {
        pin: PinRef,
        value: Expr,
    },
    PinToggle {
        pin: PinRef,
    },
    WaitPin {
        pin: PinRef,
        level: bool,
    },
    DelayMs {
        ms: Expr,
    },
    DelayUs {
        us: u32,
    },
    If {
        cond: Expr,
        then_body: Vec<Stmt>,
        else_body: Vec<Stmt>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    Repeat {
        count: Expr,
        index_var: Option<String>,
        body: Vec<Stmt>,
    },
    Call {
        function: String,
    },
    Return,
    SetVar {
        name: String,
        value: Expr,
    },
    UartSendString {
        text: String,
        newline: bool,
    },
    UartSendNumber {
        value: Expr,
        newline: bool,
    },
    UartSendByte {
        value: Expr,
    },
    UartReceiveByte {
        var: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stmt {
    /// Graph node this statement came from (used in generated comments and diagnostics).
    pub node_id: String,
    pub label: String,
    pub kind: StmtKind,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IrVariable {
    pub name: String,
    pub var_type: VarType,
    pub initial: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IrFunction {
    pub name: String,
    pub node_id: String,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtTrigger {
    Low,
    Change,
    Falling,
    Rising,
}

impl ExtTrigger {
    /// ISCn1:ISCn0 encoding shared by INT0/INT1 on both register profiles.
    pub fn isc_bits(self) -> u8 {
        match self {
            ExtTrigger::Low => 0,
            ExtTrigger::Change => 1,
            ExtTrigger::Falling => 2,
            ExtTrigger::Rising => 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IrExternalInterrupt {
    pub source: String,
    pub vector: String,
    pub trigger: ExtTrigger,
    pub node_id: String,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Uses {
    pub uart: bool,
    pub uart_number: bool,
    pub adc: bool,
    pub variable_delay: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IrProgram {
    pub variables: Vec<IrVariable>,
    pub functions: Vec<IrFunction>,
    pub external_interrupts: Vec<IrExternalInterrupt>,
    pub setup: Vec<Stmt>,
    pub loop_body: Vec<Stmt>,
    pub uses: Uses,
}
