use schemars::{JsonSchema, Schema, schema_for};
use serde::{Deserialize, Serialize, Serializer, ser::SerializeMap};

pub trait FunctionTool {
    type Args<'a>: JsonSchema + Deserialize<'a>;
    type Return: Serialize;
    const DESCRIPTION: &str;

    fn execute(&self, args: Self::Args<'_>, call_id: &str) -> (Self::Return, SideEffect);

    fn run(&self, payload: &str, call_id: &str) -> ToolResult {
        serde_json::from_str::<Self::Args<'_>>(payload)
            .map(|args| {
                let (result, side_effect) = self.execute(args, call_id);
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
    Store(TransferTarget),
    Transfer(String),
    Terminate,
}

#[derive(Debug)]
pub struct TransferTarget {
    pub call_id: String,
    pub target: String,
}

pub enum SideEffectResult {
    Ok,
    Final,
    Error(String),
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ValidateResult {
    valid: bool,
    call_id: Option<String>,
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

    fn execute(&self, args: Self::Args<'_>, call_id: &str) -> (Self::Return, SideEffect) {
        info!("ValidatePhoneNumber called digits {:?}", args.digits);
        if args.digits.len() < 7 {
            return bad_check("Number too short");
        }
        let digit_chars: Result<Vec<u8>, ()> = args
            .digits
            .iter()
            .map(|dig| {
                if *dig < 10 {
                    Ok(*dig as u8 + 48)
                } else {
                    Err(())
                }
            })
            .collect();
        match digit_chars {
            Ok(digits) => {
                let transfer = TransferTarget {
                    call_id: call_id.to_string(),
                    target: String::from_utf8_lossy(&digits).to_string(),
                };
                info!("Storing validated transfer target: {:?}", transfer);
                (
                    ValidateResult {
                        valid: true,
                        call_id: Some(call_id.to_string()),
                    },
                    SideEffect::Store(transfer),
                )
            }
            Err(_) => bad_check("Invalid digits"),
        }
    }
}

fn bad_check(reason: &str) -> (ValidateResult, SideEffect) {
    warn!("Request not valid: {}", reason);
    (
        ValidateResult {
            valid: false,
            call_id: None,
        },
        SideEffect::Noop,
    )
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

    fn execute(&self, args: Self::Args<'_>, _call_id: &str) -> (Self::Return, SideEffect) {
        info!("ValidateContact called with {:#?}", args);
        let valid = false;
        (
            ValidateResult {
                valid,
                call_id: None,
            },
            SideEffect::Noop,
        )
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TransferArgs {
    #[schemars(
        description = "The call_id returned by a previous successful call to ValidatePhoneNumber or ValidateContact"
    )]
    call_id: String,
}

#[derive(Debug, Deserialize)]
pub struct Transfer;

impl FunctionTool for Transfer {
    type Args<'a> = TransferArgs;
    type Return = ();
    const DESCRIPTION: &str = r#"Transfer the user to a previously validated number or contact.
    This MUST ONLY be used after a successful call to ValidatePhoneNumber or ValidateContact.
    Returns:
        null: On success.
        error(string): If there was an error with the function call itself. 
    "#;

    fn execute(&self, args: Self::Args<'_>, _call_id: &str) -> (Self::Return, SideEffect) {
        info!("Call transfer request: {:?}", args);
        ((), SideEffect::Transfer(args.call_id))
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TerminateArgs {
    #[schemars(description = "The reason for terminating the call")]
    reason: TerminateReason,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub enum TerminateReason {
    #[schemars(description = "User requested termination")]
    UserRequested,
    #[schemars(description = "User is uncooperative")]
    UserUncooperative,
    #[schemars(description = "Other reason. This should rarely be used")]
    Other,
}

#[derive(Debug, Deserialize)]
pub struct Terminate;

impl FunctionTool for Terminate {
    type Args<'a> = TerminateArgs;
    type Return = ();
    const DESCRIPTION: &str = r#"Terminate the session if the user requests or is uncooperative.
    Returns:
        null: On success.
        error(string): If there was an error with the function call itself. 
    "#;

    fn execute(&self, args: Self::Args<'_>, _call_id: &str) -> (Self::Return, SideEffect) {
        info!("Call termination request: {:?}", args);
        ((), SideEffect::Terminate)
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
            pub fn run(&self, payload: &str, call_id: &str) -> ToolResult {
                match self {
                    $(Self::$tool => $tool.run(payload, call_id),)*
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

build_tools!(ValidatePhoneNumber, ValidateContact, Transfer, Terminate);
