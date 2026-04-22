//! Adapters from Reflow's runtime-neutral template catalog to Zeal SDK types.

use std::collections::HashMap;

use reflow_network::template as reflow_template;
use zeal_sdk::types::{
    DisplayComponent, NodeShape, NodeSize, NodeTemplate, Port, PortPosition, PortType,
    PropertyDefinition, PropertyRules, PropertyType, PropertyValidation, RuntimeRequirements,
};

pub fn to_zeal_template(template: reflow_template::NodeTemplate) -> NodeTemplate {
    NodeTemplate {
        id: template.id,
        type_name: template.type_name,
        title: template.title,
        subtitle: template.subtitle,
        category: template.category,
        subcategory: template.subcategory,
        description: template.description,
        icon: template.icon,
        variant: template.variant,
        shape: template.shape.map(to_zeal_shape),
        size: template.size.map(to_zeal_size),
        ports: template.ports.into_iter().map(to_zeal_port).collect(),
        properties: template.properties.map(to_zeal_properties),
        property_rules: template.property_rules.and_then(to_zeal_property_rules),
        runtime: template.runtime.map(to_zeal_runtime),
        display: template.display.map(to_zeal_display),
    }
}

fn to_zeal_display(display: reflow_template::DisplayComponent) -> DisplayComponent {
    DisplayComponent {
        element: display.element,
        bundle_id: display.bundle_id,
        source: display.source,
        shadow: display.shadow,
        observed_props: display.observed_props,
        width: display.width,
    }
}

fn to_zeal_shape(shape: reflow_template::NodeShape) -> NodeShape {
    match shape {
        reflow_template::NodeShape::Rectangle => NodeShape::Rectangle,
        reflow_template::NodeShape::Circle => NodeShape::Circle,
        reflow_template::NodeShape::Diamond => NodeShape::Diamond,
    }
}

fn to_zeal_size(size: reflow_template::NodeSize) -> NodeSize {
    match size {
        reflow_template::NodeSize::Small => NodeSize::Small,
        reflow_template::NodeSize::Medium => NodeSize::Medium,
        reflow_template::NodeSize::Large => NodeSize::Large,
    }
}

fn to_zeal_port(port: reflow_template::Port) -> Port {
    Port {
        id: port.id,
        label: port.label,
        port_type: match port.port_type {
            reflow_template::PortType::Input => PortType::Input,
            reflow_template::PortType::Output => PortType::Output,
        },
        position: match port.position {
            reflow_template::PortPosition::Left => PortPosition::Left,
            reflow_template::PortPosition::Right => PortPosition::Right,
            reflow_template::PortPosition::Top => PortPosition::Top,
            reflow_template::PortPosition::Bottom => PortPosition::Bottom,
        },
        data_type: port.data_type,
        required: port.required,
        multiple: port.multiple,
    }
}

fn to_zeal_properties(
    properties: HashMap<String, reflow_template::PropertyDefinition>,
) -> HashMap<String, PropertyDefinition> {
    properties
        .into_iter()
        .map(|(name, property)| (name, to_zeal_property(property)))
        .collect()
}

fn to_zeal_property(property: reflow_template::PropertyDefinition) -> PropertyDefinition {
    PropertyDefinition {
        property_type: match property.property_type {
            reflow_template::PropertyType::String => PropertyType::String,
            reflow_template::PropertyType::Number => PropertyType::Number,
            reflow_template::PropertyType::Boolean => PropertyType::Boolean,
            reflow_template::PropertyType::Select => PropertyType::Select,
            reflow_template::PropertyType::CodeEditor => PropertyType::CodeEditor,
        },
        label: property.label,
        description: property.description,
        default_value: property.default_value,
        options: property.options,
        validation: property.validation.map(to_zeal_validation),
    }
}

fn to_zeal_property_rules(value: serde_json::Value) -> Option<PropertyRules> {
    serde_json::from_value(value).ok()
}

fn to_zeal_validation(validation: reflow_template::PropertyValidation) -> PropertyValidation {
    PropertyValidation {
        required: validation.required,
        min: validation.min,
        max: validation.max,
        pattern: validation.pattern,
    }
}

fn to_zeal_runtime(runtime: reflow_template::RuntimeRequirements) -> RuntimeRequirements {
    RuntimeRequirements {
        executor: runtime.executor,
        version: runtime.version,
        required_env_vars: runtime.required_env_vars,
        capabilities: runtime.capabilities,
    }
}
