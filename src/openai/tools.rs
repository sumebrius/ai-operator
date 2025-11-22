use schemars::{JsonSchema, Schema, schema_for};
use serde::{Deserialize, Serialize, Serializer, ser::SerializeMap};

pub trait FunctionTool {
    type Args<'a>: JsonSchema + Deserialize<'a>;
    type Return: Serialize;
    const DESCRIPTION: &str;

    fn execute(&self, args: Self::Args<'_>) -> (Self::Return, SideEffect);

    fn run(&self, payload: &str) -> ToolResult {
        serde_json::from_str::<Self::Args<'_>>(payload)
            .map(|args| {
                let (result, side_effect) = self.execute(args);
                match serde_json::to_string(&result) {
                    Ok(output) => ToolResult {
                        output,
                        side_effect,
                    },
                    Err(err) => err.into(),
                }
            })
            .unwrap_or_else(|err| err.into())
    }

    fn parameters() -> Schema {
        schema_for!(Self::Args<'_>)
    }
}

pub struct ToolResult {
    pub output: String,
    pub side_effect: SideEffect,
}

impl From<serde_json::Error> for ToolResult {
    fn from(value: serde_json::Error) -> Self {
        let output = serde_json::json!({"error": format!("{:?}", value)}).to_string();
        Self {
            output,
            side_effect: SideEffect::Noop,
        }
    }
}

pub enum SideEffect {
    Noop,
    Transfer(String),
    Terminate,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ValidateResult {
    valid: bool,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ValidatePhoneNumberArgs {
    #[schemars(description = "An array of individual digits of the phone number to check")]
    digits: Vec<usize>,
}

#[derive(Debug, Deserialize)]
pub struct ValidatePhoneNumber;

impl FunctionTool for ValidatePhoneNumber {
    type Args<'a> = ValidatePhoneNumberArgs;
    type Return = ValidateResult;
    const DESCRIPTION: &str = r#"Validate a phone number is valid and can be transferred to.
    Returns:
        valid(bool): If the number is valid and can be transferred to.
        error(string): If there was an error with the function call itself. 
    "#;

    fn execute(&self, args: Self::Args<'_>) -> (Self::Return, SideEffect) {
        info!("ValidatePhoneNumber called with {:#?}", args);
        let valid = args.digits.len() > 4;
        (ValidateResult { valid }, SideEffect::Noop)
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ValidateContactArgs {
    #[schemars(description = "An array of contact names to check")]
    contact: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ValidateContact;

impl FunctionTool for ValidateContact {
    type Args<'a> = ValidateContactArgs;
    type Return = ValidateResult;
    const DESCRIPTION: &str = r#"Validate a contact is known and can be transferred to.
    Returns:
        valid(bool): If the number is valid and can be transferred to.
        error(string): If there was an error with the function call itself. 
    "#;

    fn execute(&self, args: Self::Args<'_>) -> (Self::Return, SideEffect) {
        info!("ValidateContact called with {:#?}", args);
        let valid = true;
        (ValidateResult { valid }, SideEffect::Noop)
    }
}

macro_rules! build_tools {
    ( $( $tool:ident ),+ ) => {

        #[derive(Debug, Deserialize)]
        #[serde(tag = "name")]
        pub enum Tool {
            $($tool,)*
        }

        impl Tool {
            pub fn run(&self, payload: &str) -> ToolResult {
                match self {
                    $(Self::$tool => $tool.run(payload),)*
                }
            }

            pub fn all() -> Vec<Self> {
                vec![$(Tool::$tool, )*]
            }
        }

        impl Serialize for Tool {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                let (name, description, parameters) = match self {
                    $(Tool::$tool => {
                        (stringify!($tool), $tool::DESCRIPTION, &$tool::parameters())
                    },)*
                };

                let mut ser = serializer.serialize_map(Some(4))?;
                ser.serialize_entry("type", "function")?;
                ser.serialize_entry("name", name)?;
                ser.serialize_entry("description", description)?;
                ser.serialize_entry("parameters", parameters)?;
                ser.end()
            }
        }

    };
}

build_tools!(ValidatePhoneNumber, ValidateContact);
