/// Tools for use by the model as function calls
/// The layers are leaky asf for this
/// We define how to run it and the *first* layer of serialising the result here
/// The run function is called by the `FunctionCall` event in server events,
///   which mixes that in with its own deets to generate the client event.
/// And the call of that is done in the innermost loop of the main control thread.
use serde::{Deserialize, Serialize, Serializer, ser::SerializeMap};
use serde_json::{Value, json};

use crate::contacts::ContactList;

pub trait FunctionTool {
    /// Args taken by the tool. We generate a schema to tell the model how to call it.
    /// Note that this always needs to be a full struct, otherwise the API gets thoroughly
    /// confused and times out. Use `NullArgs` if no args are required.
    type Args<'a>: Deserialize<'a>;
    /// The Return type of the tool's payload back to the model.
    type Return: Serialize;
    /// Description of the tool. Used to describe it to the model, so make it prompty.
    const DESCRIPTION: &str;

    /// The actual logic for individual tools
    fn execute(&self, args: Self::Args<'_>, call_id: &str) -> (Self::Return, SideEffect);

    /// External interface for the trait.
    /// Deserialises the args, calls `.execute()`, and serialises the response
    /// while handling errors
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

    /// Just here for serialisation
    fn parameters() -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }))
    }
}

pub struct ToolResult {
    pub output: String,
    pub side_effect: SideEffect,
}

impl From<serde_json::Error> for ToolResult {
    fn from(value: serde_json::Error) -> Self {
        error!("Error de/serialising tool call args/result: {}", value);
        let output = serde_json::json!({"error": format!("{:?}", value)}).to_string();
        Self {
            output,
            side_effect: SideEffect::Noop,
        }
    }
}

/// Side effect of a tool call
pub enum SideEffect {
    /// No side effects
    Noop,
    /// Successful validation - store the details for a later transfer
    Store(TransferTarget),
    /// Transfer to an earlier validated target
    Transfer(String),
    /// Hang up the call
    Terminate,
}

/// A validated number for transferring to
/// These get stored in a vec, and grab one by the call_id when transferring.
/// Basically a hashmap without the hash, 'cos we'll only ever have like 2 of
/// these, tops.
/// The model only ever knows about the call_id, so we can be safe against
/// hallucinations in the actual transfer call.
#[derive(Debug)]
pub struct TransferTarget {
    /// The call id of the tool call that validated/stored this
    pub call_id: String,
    /// The number to transfer to.
    pub target: String,
}

/// The result of calling the side effect by the controller.
/// Basically a ternary Result
pub enum SideEffectResult {
    /// OK, and Keep going
    Ok,
    /// OK, but terminate the thread
    Final,
    /// Error with the side effect itself
    /// Report back to the model and keep going.
    Error(String),
}

#[derive(Debug, Deserialize)]
pub struct NullArgs {}

#[derive(Debug, Serialize)]
pub struct ValidateResult {
    valid: bool,
    call_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ValidatePhoneNumberArgs {
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

    /// Currently just validates it has at least enough digits.
    /// At some stgae, we prolly want to validate this against a dial plan...
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

    fn parameters() -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "digits": {
                    "type": "array",
                    "items": { "type": "integer", "minimum": 0 },
                    "description": "An array of individual digits of the phone number to check"
                }
            },
            "required": ["digits"],
            "additionalProperties": false
        }))
    }
}

/// Basically just ? for the above tool's execute.
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

#[derive(Debug, Deserialize)]
pub struct ValidateContactArgs {
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

    /// Currently just loads the contact list from a static CSV which we load on each call.
    /// Is there a better way to do this? Yes, but cbf...
    fn execute(&self, args: Self::Args<'_>, call_id: &str) -> (Self::Return, SideEffect) {
        info!("ValidateContact for contacts: {:?}", args.contact);
        let contact_list = ContactList::from_default();
        for name in args.contact {
            if let Some(contact) = contact_list.find(&name) {
                let transfer = TransferTarget {
                    call_id: call_id.to_string(),
                    target: contact.number().to_string(),
                };
                info!("Storing validated transfer target: {:?}", transfer);
                return (
                    ValidateResult {
                        valid: true,
                        call_id: Some(call_id.to_string()),
                    },
                    SideEffect::Store(transfer),
                );
            }
        }
        // Fallthrough - nothing found
        (
            ValidateResult {
                valid: false,
                call_id: None,
            },
            SideEffect::Noop,
        )
    }

    fn parameters() -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "contact": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "An array of contact names to check. If a location is specified, add it to the end separated by a space. eg. \"Bob Office\""
                }
            },
            "required": ["contact"],
            "additionalProperties": false
        }))
    }
}

#[derive(Debug, Deserialize)]
pub struct ListContacts;

impl FunctionTool for ListContacts {
    type Args<'a> = NullArgs;
    type Return = Vec<String>;
    const DESCRIPTION: &str = r#"Get a list of all available contact names.
    Use this if you want to cross-check a name provided by the user if you are not sure of
    spelling, or a previous validation has return false.
    Note that this DOES NOT REPLACE the need to call ValidateContact.
    Returns:
        array(str): List of available contacts
        error(string): If there was an error with the function call itself. 
    "#;

    fn execute(&self, _args: Self::Args<'_>, _call_id: &str) -> (Self::Return, SideEffect) {
        (ContactList::from_default().list(), SideEffect::Noop)
    }

    fn parameters() -> Option<Value> {
        None
    }
}

#[derive(Debug, Deserialize)]
pub struct TransferArgs {
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

    fn parameters() -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "call_id": {
                    "type": "string",
                    "description": "The call_id returned by a previous successful call to ValidatePhoneNumber or ValidateContact"
                }
            },
            "required": ["call_id"],
            "additionalProperties": false
        }))
    }
}

#[derive(Debug, Deserialize)]
pub struct TerminateArgs {
    reason: TerminateReason,
}

#[derive(Debug, Deserialize)]
pub enum TerminateReason {
    UserRequested,
    UserUncooperative,
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
        warn!("Call termination request: {:?}", args.reason);
        ((), SideEffect::Terminate)
    }

    fn parameters() -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "reason": {
                    "type": "string",
                    "enum": ["UserRequested", "UserUncooperative", "Other"],
                    "description": "The reason for terminating the call"
                }
            },
            "required": ["reason"],
            "additionalProperties": false
        }))
    }
}

/// This builds an enum out of all the tools.
/// It only gets called once, but each tool's name is used 8 times in the definition,
/// So fuck that noise of adding them in manually.
macro_rules! build_tools {
    ( $( $tool:ident ),+ ) => {

        #[derive(Debug, Deserialize)]
        #[serde(tag = "name")]
        pub enum Tool {
            $($tool,)*
        }

        impl Tool {
            /// Proxy through to each tool's run method.
            pub fn run(&self, payload: &str, call_id: &str) -> ToolResult {
                match self {
                    $(Self::$tool => $tool.run(payload, call_id),)*
                }
            }

            /// Get a list of all the tools to tell the model about them.
            /// Each one gets serialised so this blows out to a huge chunk of payload
            pub fn all() -> Vec<Self> {
                vec![$(Tool::$tool, )*]
            }
        }

        /// Serialise each member to this payload:
        /// https://platform.openai.com/docs/api-reference/realtime-calls/accept-call#realtime_calls_accept_call-tools-function_tool
        impl Serialize for Tool {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                let (name, description, parameters) = match self {
                    $(Tool::$tool => {
                        (stringify!($tool), $tool::DESCRIPTION, $tool::parameters())
                    },)*
                };

                let mut ser = serializer.serialize_map(Some(4))?;
                ser.serialize_entry("type", "function")?;
                ser.serialize_entry("name", name)?;
                ser.serialize_entry("description", description)?;
                ser.serialize_entry("parameters", &parameters)?;
                ser.end()
            }
        }

    };
}

build_tools!(
    ValidatePhoneNumber,
    ValidateContact,
    ListContacts,
    Transfer,
    Terminate
);
