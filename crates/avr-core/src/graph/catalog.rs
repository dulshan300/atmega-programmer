//! Block catalog shared with the frontend (`blocks/catalog.json`).

use crate::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

const CATALOG_JSON: &str = include_str!("../../../../blocks/catalog.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PortType {
    Exec,
    Bool,
    Int,
    /// Resolves to `bool` or `int` from the block's `variable` property.
    Variable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortDef {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub port_type: PortType,
    /// When unconnected, the value of this property is used instead.
    #[serde(default)]
    pub fallback_property: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyKind {
    Int,
    Bool,
    String,
    Enum,
    Pin,
    Variable,
    Identifier,
    AdcChannel,
    ExtInterrupt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnumOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertyDef {
    pub id: String,
    pub label: String,
    pub kind: PropertyKind,
    pub default: Value,
    #[serde(default)]
    pub min: Option<i64>,
    #[serde(default)]
    pub max: Option<i64>,
    #[serde(default)]
    pub max_length: Option<usize>,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub options: Vec<EnumOption>,
    #[serde(default)]
    pub true_label: Option<String>,
    #[serde(default)]
    pub false_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlockDef {
    #[serde(rename = "type")]
    pub block_type: String,
    pub category: String,
    pub label: String,
    pub description: String,
    #[serde(default)]
    pub unique: bool,
    pub inputs: Vec<PortDef>,
    pub outputs: Vec<PortDef>,
    pub properties: Vec<PropertyDef>,
}

impl BlockDef {
    pub fn input(&self, id: &str) -> Option<&PortDef> {
        self.inputs.iter().find(|p| p.id == id)
    }
    pub fn output(&self, id: &str) -> Option<&PortDef> {
        self.outputs.iter().find(|p| p.id == id)
    }
    pub fn property(&self, id: &str) -> Option<&PropertyDef> {
        self.properties.iter().find(|p| p.id == id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CategoryDef {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogFile {
    schema_version: u32,
    categories: Vec<CategoryDef>,
    blocks: Vec<BlockDef>,
}

pub struct BlockCatalog {
    blocks: BTreeMap<String, BlockDef>,
}

impl BlockCatalog {
    pub fn builtin() -> CoreResult<Self> {
        let file: CatalogFile = serde_json::from_str(CATALOG_JSON)
            .map_err(|e| CoreError::InvalidRequest(format!("block catalog: {e}")))?;
        let mut blocks = BTreeMap::new();
        for b in file.blocks {
            if !file.categories.iter().any(|c| c.id == b.category) {
                return Err(CoreError::InvalidRequest(format!(
                    "block {} has unknown category",
                    b.block_type
                )));
            }
            for p in &b.inputs {
                if let Some(fp) = &p.fallback_property
                    && b.property(fp).is_none()
                {
                    return Err(CoreError::InvalidRequest(format!(
                        "block {} input {} falls back to missing property {fp}",
                        b.block_type, p.id
                    )));
                }
            }
            if blocks.insert(b.block_type.clone(), b).is_some() {
                return Err(CoreError::InvalidRequest(
                    "duplicate block type in catalog".into(),
                ));
            }
        }
        Ok(BlockCatalog { blocks })
    }

    pub fn get(&self, block_type: &str) -> Option<&BlockDef> {
        self.blocks.get(block_type)
    }
}
