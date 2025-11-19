use schemars::{JsonSchema, Schema, schema_for};
use serde::{Deserialize, Serialize, Serializer, ser::SerializeMap};

pub trait FunctionTool {
    type Args<'a>: JsonSchema + Deserialize<'a>;
    type Return: Serialize;
    const DESCRIPTION: &str;

    fn execute(&self, args: Self::Args<'_>) -> Self::Return;

    fn run(&self, payload: &str) -> String {
        // Deserialise args
        serde_json::from_str::<Self::Args<'_>>(payload)
            // Execute
            .map(|args| self.execute(args))
            // Serialise result
            .and_then(|result| serde_json::to_string(&result))
            // Serialise any serde errors
            .unwrap_or_else(|err| serde_json::json!({"error": format!("{:?}", err)}).to_string())
    }

    fn description(&self) -> &str {
        Self::DESCRIPTION
    }
    fn parameters(&self) -> Schema {
        schema_for!(Self::Args<'_>)
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "name")]
pub enum Tool {
    ToolA,
}

impl Tool {
    pub fn get_tool(&self) -> impl FunctionTool {
        match self {
            Tool::ToolA => ToolA,
        }
    }
    pub fn run(&self, payload: &str) -> String {
        self.get_tool().run(payload)
    }

    pub fn all() -> Vec<Self> {
        vec![Self::ToolA]
    }
}

impl Serialize for Tool {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let tool = self.get_tool();
        let name = match self {
            Tool::ToolA => "ToolA",
        };

        let mut ser = serializer.serialize_map(Some(4))?;
        ser.serialize_entry("type", "function")?;
        ser.serialize_entry("name", name)?;
        ser.serialize_entry("description", tool.description())?;
        ser.serialize_entry("parameters", &tool.parameters())?;
        ser.end()
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ToolArgs {
    phrase: String,
    sentiment: String,
    digits: Option<Vec<usize>>,
}

#[derive(Debug, Deserialize)]
pub struct ToolA;

impl FunctionTool for ToolA {
    type Args<'a> = ToolArgs;
    type Return = usize;
    const DESCRIPTION: &str = r#"Use this tool if the user says the magic word 'chicken'.
    If they say any digits after saying chicken, include this in the function call. Ensure digits are passed as an array of individual single digits.
    Give the user the response directly.
    "#;

    fn execute(&self, args: Self::Args<'_>) -> Self::Return {
        info!("Tool called with {:#?}", args);
        69
    }
}
