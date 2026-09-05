//! The package's parameter schema against the plugin's own table.
//!
//! RackForge draws controls from `metadata/parameters.json` and RF-7 answers
//! to its own [`parameters::PARAMETERS`]. Nothing at run time forces the two to
//! agree, so a parameter added to one and forgotten in the other would reach a
//! surface as a control that moves nothing. This is the only place that
//! notices, so it is deliberately literal about every field.

use rf7_plugin::parameters::{self, Kind};
use serde_json::Value;
use std::collections::BTreeSet;

const SCHEMA: &str = include_str!("../../../package/metadata/parameters.json");

fn schema() -> Value {
    serde_json::from_str(SCHEMA).expect("the package schema must be valid JSON")
}

#[test]
fn the_schema_declares_exactly_the_parameters_the_plugin_answers_to() {
    let schema = schema();
    let declared = schema["parameters"]
        .as_array()
        .expect("parameters must be an array");
    assert_eq!(
        declared.len(),
        parameters::COUNT,
        "the package and the plugin disagree about how many parameters exist"
    );

    for (index, parameter) in parameters::PARAMETERS.iter().enumerate() {
        let entry = &declared[index];
        assert_eq!(
            entry["index"].as_u64(),
            Some(index as u64),
            "{} is not at index {index}",
            parameter.id
        );
        assert_eq!(
            entry["id"].as_str(),
            Some(parameter.id),
            "index {index} is named differently in the package"
        );
        let kind = &entry["kind"];
        let declared_type = kind["type"].as_str().expect("every kind has a type");
        match parameter.kind {
            Kind::Float => {
                assert_eq!(declared_type, "float", "{}", parameter.id);
                assert_eq!(
                    kind["minimum"].as_f64(),
                    Some(parameter.minimum),
                    "{}",
                    parameter.id
                );
                assert_eq!(
                    kind["maximum"].as_f64(),
                    Some(parameter.maximum),
                    "{}",
                    parameter.id
                );
                assert_eq!(
                    kind["default"].as_f64(),
                    Some(parameter.default),
                    "{}",
                    parameter.id
                );
            }
            Kind::Integer => {
                assert_eq!(declared_type, "integer", "{}", parameter.id);
                assert_eq!(
                    kind["minimum"].as_f64(),
                    Some(parameter.minimum),
                    "{}",
                    parameter.id
                );
                assert_eq!(
                    kind["maximum"].as_f64(),
                    Some(parameter.maximum),
                    "{}",
                    parameter.id
                );
                assert_eq!(
                    kind["default"].as_f64(),
                    Some(parameter.default),
                    "{}",
                    parameter.id
                );
            }
            Kind::Boolean => {
                assert_eq!(declared_type, "boolean", "{}", parameter.id);
                let default = kind["default"].as_bool().expect("a boolean default");
                assert_eq!(
                    f64::from(u8::from(default)),
                    parameter.default,
                    "{} starts differently in the package",
                    parameter.id
                );
            }
            Kind::Enum => {
                assert_eq!(declared_type, "enum", "{}", parameter.id);
                let choices = kind["choices"].as_array().expect("an enum has choices");
                assert_eq!(
                    choices.len() as f64,
                    parameter.maximum + 1.0,
                    "{} offers a different number of choices",
                    parameter.id
                );
                assert_eq!(
                    kind["default"].as_f64(),
                    Some(parameter.default),
                    "{}",
                    parameter.id
                );
                for (value, choice) in choices.iter().enumerate() {
                    assert_eq!(choice["value"].as_u64(), Some(value as u64));
                    assert!(!choice["name"].as_str().unwrap_or("").trim().is_empty());
                }
            }
        }
    }
}

#[test]
fn every_parameter_names_a_page_the_schema_declares() {
    let schema = schema();
    let pages: BTreeSet<&str> = schema["pages"]
        .as_array()
        .expect("pages must be an array")
        .iter()
        .map(|page| page["id"].as_str().expect("a page has an id"))
        .collect();
    assert!(!pages.is_empty());
    let mut used = BTreeSet::new();
    for parameter in schema["parameters"].as_array().expect("an array") {
        let page = parameter["page"].as_str().expect("a parameter has a page");
        assert!(
            pages.contains(page),
            "{} names the page {page}, which is not declared",
            parameter["id"]
        );
        used.insert(page);
    }
    assert_eq!(used, pages, "a declared page holds no parameters");
}

#[test]
fn every_parameter_is_automatable_and_writable() {
    // A read-only or non-automatable control here would be a mistake rather
    // than a decision: all seventeen are things a player or a sequencer moves.
    for parameter in schema()["parameters"].as_array().expect("an array") {
        let flags = &parameter["flags"];
        assert_eq!(
            flags["automatable"].as_bool(),
            Some(true),
            "{}",
            parameter["id"]
        );
        assert_eq!(
            flags["read_only"].as_bool(),
            Some(false),
            "{}",
            parameter["id"]
        );
    }
}

#[test]
fn the_orders_within_a_page_are_distinct() {
    let schema = schema();
    let mut seen = BTreeSet::new();
    for parameter in schema["parameters"].as_array().expect("an array") {
        let page = parameter["page"].as_str().expect("a page");
        let order = parameter["order"].as_u64().expect("an order");
        assert!(
            seen.insert((page.to_owned(), order)),
            "{} shares its position with another control on {page}",
            parameter["id"]
        );
    }
}
