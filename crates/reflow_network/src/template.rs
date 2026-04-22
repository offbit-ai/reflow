//! Runtime-neutral actor template metadata.
//!
//! Component crates can publish editor/display metadata through these types
//! without depending on a particular editor SDK. Integrations such as
//! `reflow_server` adapt this catalog to Zeal, web UIs, or other authoring
//! surfaces at their boundary.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PortType {
    Input,
    Output,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PortPosition {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeShape {
    Rectangle,
    Circle,
    Diamond,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeSize {
    Small,
    Medium,
    Large,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PropertyType {
    String,
    Number,
    Boolean,
    Select,
    CodeEditor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyValidation {
    pub required: Option<bool>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub pattern: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyDefinition {
    #[serde(rename = "type")]
    pub property_type: PropertyType,
    pub label: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "defaultValue")]
    pub default_value: Option<Value>,
    pub options: Option<Vec<Value>>,
    pub validation: Option<PropertyValidation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Port {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub port_type: PortType,
    pub position: PortPosition,
    #[serde(rename = "dataType")]
    pub data_type: Option<String>,
    pub required: Option<bool>,
    pub multiple: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeRequirements {
    pub executor: String,
    pub version: Option<String>,
    #[serde(rename = "requiredEnvVars")]
    pub required_env_vars: Option<Vec<String>>,
    pub capabilities: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayComponent {
    pub element: String,
    #[serde(rename = "bundleId")]
    pub bundle_id: Option<String>,
    pub source: Option<String>,
    pub shadow: Option<bool>,
    #[serde(rename = "observedProps")]
    pub observed_props: Option<Vec<String>>,
    pub width: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeTemplate {
    pub id: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub category: String,
    pub subcategory: Option<String>,
    pub description: String,
    pub icon: String,
    pub variant: Option<String>,
    pub shape: Option<NodeShape>,
    pub size: Option<NodeSize>,
    pub ports: Vec<Port>,
    pub properties: Option<HashMap<String, PropertyDefinition>>,
    #[serde(rename = "propertyRules")]
    pub property_rules: Option<Value>,
    pub runtime: Option<RuntimeRequirements>,
    pub display: Option<DisplayComponent>,
}

/// A crate-owned catalog of templates and display bundles.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateCatalog {
    pub templates: Vec<NodeTemplate>,
    pub display_components: Vec<DisplayComponentSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayComponentSource {
    pub element: String,
    pub source: String,
}

/// In-memory registration surface for crate-owned actor template catalogs.
#[derive(Debug, Clone, Default)]
pub struct TemplateRegistry {
    catalog: TemplateCatalog,
}

impl TemplateRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_catalog(&mut self, catalog: TemplateCatalog) {
        for template in catalog.templates {
            self.register_template(template);
        }
        for component in catalog.display_components {
            self.register_display_component(component);
        }
    }

    pub fn register_template(&mut self, template: NodeTemplate) {
        if let Some(existing) = self
            .catalog
            .templates
            .iter_mut()
            .find(|existing| existing.id == template.id)
        {
            *existing = template;
        } else {
            self.catalog.templates.push(template);
        }
    }

    pub fn register_display_component(&mut self, component: DisplayComponentSource) {
        if let Some(existing) = self
            .catalog
            .display_components
            .iter_mut()
            .find(|existing| existing.element == component.element)
        {
            *existing = component;
        } else {
            self.catalog.display_components.push(component);
        }
    }

    pub fn templates(&self) -> &[NodeTemplate] {
        &self.catalog.templates
    }

    pub fn display_components(&self) -> &[DisplayComponentSource] {
        &self.catalog.display_components
    }

    pub fn into_catalog(self) -> TemplateCatalog {
        self.catalog
    }
}
