use anyhow::{Result, anyhow};
use async_trait::async_trait;
use context_server::{Tool, ToolContent, ToolExecutor};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug)]
enum UnitType {
    Distance,
    Volume,
    Weight,
    Temperature,
    Digital,
    Pressure,
    Speed,
}

impl std::fmt::Display for UnitType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnitType::Distance => write!(f, "distance"),
            UnitType::Volume => write!(f, "volume"),
            UnitType::Weight => write!(f, "weight"),
            UnitType::Temperature => write!(f, "temperature"),
            UnitType::Digital => write!(f, "digital"),
            UnitType::Pressure => write!(f, "pressure"),
            UnitType::Speed => write!(f, "speed"),
        }
    }
}

#[derive(Deserialize, JsonSchema, Serialize)]
struct UnitConversionParams {
    #[schemars(description = "The value to convert")]
    value: f64,
    #[schemars(
        description = "The unit to convert from (e.g., meters, kilometers, miles, feet, inches, yards, nautical_miles, liters, gallons, kilograms, pounds, celsius, fahrenheit, bytes, bits, pascal, psi, mph, kph, knots, beaufort)"
    )]
    from_unit: String,
    #[schemars(
        description = "The target unit to convert to (e.g., meters, kilometers, miles, feet, inches, yards, nautical_miles, liters, gallons, kilograms, pounds, celsius, fahrenheit, bytes, bits, pascal, psi, mph, kph, knots, beaufort)"
    )]
    to_unit: String,
}

pub struct UnitConversion;

impl Default for UnitConversion {
    fn default() -> Self {
        Self::new()
    }
}

impl UnitConversion {
    pub const fn new() -> Self {
        Self
    }

    const fn beaufort_to_mps(beaufort: f64) -> f64 {
        match beaufort as i32 {
            0 => 0.0,
            1 => 1.5,
            2 => 3.0,
            3 => 5.0,
            4 => 7.5,
            5 => 10.0,
            6 => 12.5,
            7 => 15.5,
            8 => 18.5,
            9 => 22.0,
            10 => 26.0,
            11 => 30.0,
            12 => 35.0,
            _ => 35.0, // Cap at hurricane force
        }
    }

    const fn mps_to_beaufort(mps: f64) -> f64 {
        if mps < 0.5 {
            0.0
        } else if mps < 2.0 {
            1.0
        } else if mps < 4.0 {
            2.0
        } else if mps < 6.0 {
            3.0
        } else if mps < 9.0 {
            4.0
        } else if mps < 11.0 {
            5.0
        } else if mps < 14.0 {
            6.0
        } else if mps < 17.0 {
            7.0
        } else if mps < 21.0 {
            8.0
        } else if mps < 24.0 {
            9.0
        } else if mps < 28.0 {
            10.0
        } else if mps < 33.0 {
            11.0
        } else {
            12.0
        }
    }

    fn to_base_unit(value: f64, unit: &str) -> Result<(f64, UnitType)> {
        let unit_lower = unit.to_lowercase();
        match unit_lower.as_str() {
            // Distance units (to meters)
            "meters" | "m" => Ok((value, UnitType::Distance)),
            "kilometers" | "km" => Ok((value * 1000.0, UnitType::Distance)),
            "miles" | "mi" => Ok((value * 1609.344, UnitType::Distance)),
            "feet" | "ft" => Ok((value * 0.3048, UnitType::Distance)),
            "inches" | "in" => Ok((value * 0.0254, UnitType::Distance)),
            "yards" | "yd" => Ok((value * 0.9144, UnitType::Distance)),
            "nautical_miles" | "nmi" => Ok((value * 1852.0, UnitType::Distance)),

            // Volume units (to liters)
            "liters" | "l" => Ok((value, UnitType::Volume)),
            "milliliters" | "ml" => Ok((value / 1000.0, UnitType::Volume)),
            "gallons" | "gal" => Ok((value * 3.78541, UnitType::Volume)),
            "quarts" | "qt" => Ok((value * 0.946353, UnitType::Volume)),
            "pints" | "pt" => Ok((value * 0.473176, UnitType::Volume)),
            "cups" => Ok((value * 0.236588, UnitType::Volume)),
            "fluid_ounces" | "fl_oz" => Ok((value * 0.0295735, UnitType::Volume)),

            // Weight units (to kilograms)
            "kilograms" | "kg" => Ok((value, UnitType::Weight)),
            "grams" | "g" => Ok((value / 1000.0, UnitType::Weight)),
            "pounds" | "lb" | "lbs" => Ok((value * 0.453592, UnitType::Weight)),
            "ounces" | "oz" => Ok((value * 0.0283495, UnitType::Weight)),
            "stones" | "st" => Ok((value * 6.35029, UnitType::Weight)),

            // Temperature units (to celsius)
            "celsius" | "c" => Ok((value, UnitType::Temperature)),
            "fahrenheit" | "f" => Ok(((value - 32.0) * 5.0 / 9.0, UnitType::Temperature)),
            "kelvin" | "k" => Ok((value - 273.15, UnitType::Temperature)),

            // Digital units (to bytes)
            "bytes" | "b" => Ok((value, UnitType::Digital)),
            "kilobytes" | "kb" => Ok((value * 1024.0, UnitType::Digital)),
            "megabytes" | "mb" => Ok((value * 1024.0 * 1024.0, UnitType::Digital)),
            "gigabytes" | "gb" => Ok((value * 1024.0 * 1024.0 * 1024.0, UnitType::Digital)),
            "terabytes" | "tb" => {
                Ok((value * 1024.0 * 1024.0 * 1024.0 * 1024.0, UnitType::Digital))
            }
            "bits" => Ok((value / 8.0, UnitType::Digital)),
            "kilobits" | "kbit" => Ok((value * 1024.0 / 8.0, UnitType::Digital)),
            "megabits" | "mbit" => Ok((value * 1024.0 * 1024.0 / 8.0, UnitType::Digital)),
            "gigabits" | "gbit" => Ok((value * 1024.0 * 1024.0 * 1024.0 / 8.0, UnitType::Digital)),

            // Pressure units (to pascal)
            "pascal" | "pa" => Ok((value, UnitType::Pressure)),
            "kilopascal" | "kpa" => Ok((value * 1000.0, UnitType::Pressure)),
            "megapascal" | "mpa" => Ok((value * 1_000_000.0, UnitType::Pressure)),
            "bar" => Ok((value * 100_000.0, UnitType::Pressure)),
            "psi" => Ok((value * 6894.76, UnitType::Pressure)),
            "atmosphere" | "atm" => Ok((value * 101_325.0, UnitType::Pressure)),
            "torr" => Ok((value * 133.322, UnitType::Pressure)),
            "mmhg" => Ok((value * 133.322, UnitType::Pressure)),

            // Speed units (to meters per second)
            "meters_per_second" | "mps" | "m/s" => Ok((value, UnitType::Speed)),
            "kilometers_per_hour" | "kph" | "km/h" => Ok((value / 3.6, UnitType::Speed)),
            "miles_per_hour" | "mph" => Ok((value * 0.44704, UnitType::Speed)),
            "knots" | "kt" => Ok((value * 0.514444, UnitType::Speed)),
            "feet_per_second" | "fps" | "ft/s" => Ok((value * 0.3048, UnitType::Speed)),
            "beaufort" => Ok((Self::beaufort_to_mps(value), UnitType::Speed)),

            _ => Err(anyhow!("Unsupported unit: {}", unit)),
        }
    }

    fn from_base_unit(value: f64, unit: &str, unit_type: UnitType) -> Result<f64> {
        let unit_lower = unit.to_lowercase();
        match (unit_lower.as_str(), unit_type) {
            // Distance units (from meters)
            ("meters" | "m", UnitType::Distance) => Ok(value),
            ("kilometers" | "km", UnitType::Distance) => Ok(value / 1000.0),
            ("miles" | "mi", UnitType::Distance) => Ok(value / 1609.344),
            ("feet" | "ft", UnitType::Distance) => Ok(value / 0.3048),
            ("inches" | "in", UnitType::Distance) => Ok(value / 0.0254),
            ("yards" | "yd", UnitType::Distance) => Ok(value / 0.9144),
            ("nautical_miles" | "nmi", UnitType::Distance) => Ok(value / 1852.0),

            // Volume units (from liters)
            ("liters" | "l", UnitType::Volume) => Ok(value),
            ("milliliters" | "ml", UnitType::Volume) => Ok(value * 1000.0),
            ("gallons" | "gal", UnitType::Volume) => Ok(value / 3.78541),
            ("quarts" | "qt", UnitType::Volume) => Ok(value / 0.946353),
            ("pints" | "pt", UnitType::Volume) => Ok(value / 0.473176),
            ("cups", UnitType::Volume) => Ok(value / 0.236588),
            ("fluid_ounces" | "fl_oz", UnitType::Volume) => Ok(value / 0.0295735),

            // Weight units (from kilograms)
            ("kilograms" | "kg", UnitType::Weight) => Ok(value),
            ("grams" | "g", UnitType::Weight) => Ok(value * 1000.0),
            ("pounds" | "lb" | "lbs", UnitType::Weight) => Ok(value / 0.453592),
            ("ounces" | "oz", UnitType::Weight) => Ok(value / 0.0283495),
            ("stones" | "st", UnitType::Weight) => Ok(value / 6.35029),

            // Temperature units (from celsius)
            ("celsius" | "c", UnitType::Temperature) => Ok(value),
            ("fahrenheit" | "f", UnitType::Temperature) => Ok(value * 9.0 / 5.0 + 32.0),
            ("kelvin" | "k", UnitType::Temperature) => Ok(value + 273.15),

            // Digital units (from bytes)
            ("bytes" | "b", UnitType::Digital) => Ok(value),
            ("kilobytes" | "kb", UnitType::Digital) => Ok(value / 1024.0),
            ("megabytes" | "mb", UnitType::Digital) => Ok(value / (1024.0 * 1024.0)),
            ("gigabytes" | "gb", UnitType::Digital) => Ok(value / (1024.0 * 1024.0 * 1024.0)),
            ("terabytes" | "tb", UnitType::Digital) => {
                Ok(value / (1024.0 * 1024.0 * 1024.0 * 1024.0))
            }
            ("bits", UnitType::Digital) => Ok(value * 8.0),
            ("kilobits" | "kbit", UnitType::Digital) => Ok(value * 8.0 / 1024.0),
            ("megabits" | "mbit", UnitType::Digital) => Ok(value * 8.0 / (1024.0 * 1024.0)),
            ("gigabits" | "gbit", UnitType::Digital) => {
                Ok(value * 8.0 / (1024.0 * 1024.0 * 1024.0))
            }

            // Pressure units (from pascal)
            ("pascal" | "pa", UnitType::Pressure) => Ok(value),
            ("kilopascal" | "kpa", UnitType::Pressure) => Ok(value / 1000.0),
            ("megapascal" | "mpa", UnitType::Pressure) => Ok(value / 1_000_000.0),
            ("bar", UnitType::Pressure) => Ok(value / 100_000.0),
            ("psi", UnitType::Pressure) => Ok(value / 6894.76),
            ("atmosphere" | "atm", UnitType::Pressure) => Ok(value / 101_325.0),
            ("torr", UnitType::Pressure) => Ok(value / 133.322),
            ("mmhg", UnitType::Pressure) => Ok(value / 133.322),

            // Speed units (from meters per second)
            ("meters_per_second" | "mps" | "m/s", UnitType::Speed) => Ok(value),
            ("kilometers_per_hour" | "kph" | "km/h", UnitType::Speed) => Ok(value * 3.6),
            ("miles_per_hour" | "mph", UnitType::Speed) => Ok(value / 0.44704),
            ("knots" | "kt", UnitType::Speed) => Ok(value / 0.514444),
            ("feet_per_second" | "fps" | "ft/s", UnitType::Speed) => Ok(value / 0.3048),
            ("beaufort", UnitType::Speed) => Ok(Self::mps_to_beaufort(value)),

            (unit_name, unit_type) => Err(anyhow!(
                "Unsupported unit: {} for type: {}",
                unit_name,
                unit_type
            )),
        }
    }
}

#[async_trait]
impl ToolExecutor for UnitConversion {
    async fn execute(&self, arguments: Option<Value>) -> Result<Vec<ToolContent>> {
        let arguments = match arguments {
            Some(args) => args,
            None => {
                return Ok(vec![ToolContent::Text {
                    text: "Error: Missing arguments for unit conversion.\n\nTo use this tool, please provide:\n- value: The numeric value to convert (e.g., 10)\n- from_unit: The source unit (e.g., \"meters\", \"pounds\", \"celsius\")\n- to_unit: The target unit (e.g., \"feet\", \"kilograms\", \"fahrenheit\")\n\nExample: {\"value\": 10, \"from_unit\": \"meters\", \"to_unit\": \"feet\"}".to_string(),
                }]);
            }
        };

        let params: UnitConversionParams = match serde_json::from_value(arguments) {
            Ok(params) => params,
            Err(error) => {
                return Ok(vec![ToolContent::Text {
                    text: format!(
                        "Error: Invalid arguments for unit conversion.\n\nParsing failed with: {}\n\nRequired parameters:\n- value: A number (e.g., 10.5)\n- from_unit: A string specifying the source unit\n- to_unit: A string specifying the target unit\n\nPlease ensure your JSON is properly formatted and includes all required fields.",
                        error
                    ),
                }]);
            }
        };

        let (base_value, unit_type) = match Self::to_base_unit(params.value, &params.from_unit) {
            Ok(result) => result,
            Err(_) => {
                return Ok(vec![ToolContent::Text {
                    text: format!(
                        "Error: Unrecognized source unit \"{}\".\n\nSupported units by category:\n\nDistance: meters, kilometers, miles, feet, inches, yards, nautical_miles\nVolume: liters, milliliters, gallons, quarts, pints, cups, fluid_ounces\nWeight: kilograms, grams, pounds, ounces, stones\nTemperature: celsius, fahrenheit, kelvin\nDigital: bytes, kilobytes, megabytes, gigabytes, terabytes, bits, kilobits, megabits, gigabits\nPressure: pascal, kilopascal, megapascal, bar, psi, atmosphere, torr, mmhg\nSpeed: meters_per_second, kilometers_per_hour, miles_per_hour, knots, feet_per_second, beaufort\n\nNote: Units are case-insensitive. Try using the full unit name or common abbreviations.",
                        params.from_unit
                    ),
                }]);
            }
        };

        let result = match Self::from_base_unit(base_value, &params.to_unit, unit_type) {
            Ok(result) => result,
            Err(_) => {
                return Ok(vec![ToolContent::Text {
                    text: format!(
                        "Error: Cannot convert from {} ({}) to \"{}\".\n\nThe target unit \"{}\" is either:\n1. Not supported for {} conversions\n2. From a different unit category\n3. Misspelled\n\nSupported {} units: {}\n\nNote: You can only convert between units of the same type (e.g., distance to distance, weight to weight).",
                        params.from_unit,
                        unit_type,
                        params.to_unit,
                        params.to_unit,
                        unit_type,
                        unit_type,
                        match unit_type {
                            UnitType::Distance =>
                                "meters, kilometers, miles, feet, inches, yards, nautical_miles",
                            UnitType::Volume =>
                                "liters, milliliters, gallons, quarts, pints, cups, fluid_ounces",
                            UnitType::Weight => "kilograms, grams, pounds, ounces, stones",
                            UnitType::Temperature => "celsius, fahrenheit, kelvin",
                            UnitType::Digital =>
                                "bytes, kilobytes, megabytes, gigabytes, terabytes, bits, kilobits, megabits, gigabits",
                            UnitType::Pressure =>
                                "pascal, kilopascal, megapascal, bar, psi, atmosphere, torr, mmhg",
                            UnitType::Speed =>
                                "meters_per_second, kilometers_per_hour, miles_per_hour, knots, feet_per_second, beaufort",
                        }
                    ),
                }]);
            }
        };

        let response_json = json!({
            "original": format!("{} {}", params.value, params.from_unit),
            "converted": format!("{} {}", result, params.to_unit),
            "value": result,
            "unit_type": unit_type.to_string()
        });

        Ok(vec![ToolContent::Text {
            text: response_json.to_string(),
        }])
    }

    fn to_tool(&self) -> Tool {
        Tool {
            name: "unit_conversion".to_string(),
            description: Some("Convert between different units including distance (meters, kilometers, miles, feet, inches, yards, nautical_miles), volume (liters, milliliters, gallons, quarts, pints, cups, fluid ounces), weight (kilograms, grams, pounds, ounces, stones), temperature (celsius, fahrenheit, kelvin), digital storage (bytes, kilobytes, megabytes, gigabytes, terabytes, bits, kilobits, megabits, gigabits), pressure (pascal, kilopascal, megapascal, bar, psi, atmosphere, torr, mmhg), and speed (meters_per_second, kilometers_per_hour, miles_per_hour, knots, feet_per_second, beaufort)".to_string()),
            input_schema: schema_for!(UnitConversionParams).to_value(),
        }
    }
}
