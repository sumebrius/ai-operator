# AI Phone Operator

Do you run a SIP PABX? Do you think the invention of the Stowager switch was a mistake? Do you want to use modern technology to provide a ridiculously outdated user experience?

AI Operator is a LLM-powered phone operator for directing calls for SIP platforms - no dialling required!

## Setup

To set up, you'll need a few things:

1. A way to host the application somewhere that can receive HTTP requests from the public internet (see the hosting section below).
2. An OpenAI API Key (and credits to use it)
3. An OpenAI [webhook](https://developers.openai.com/api/docs/guides/webhooks) created, pointed to your URL for step 1, and subscribed to the `realtime.call.incoming` event.
4. A PBX or other SIP system to make calls from.

## Hosting

The application can be served in a number of ways:

1. A standalone binary - this is a basic rust application that can be compiled and located in your $PATH
2. A docker image - built with the Containerfile in the repo.
3. A helm chart - see the chart/ directory in this repo. Note that you will need to build the image to a registry of your choice, and update the chart values accordingly.

Regardless of the hosting method, you'll need 3 environment variables defined to run the application (the chart assumes you have a pre-existing `ai-operator` secret with these defined.):

- `OPENAI_API_KEY`
- `OPENAI_WEBHOOK_SECRET` - The secret associated with the webhook set up for the app
- `SIP_REALM` - The SIP realm for your PBX. This will be used to construct the SIP URI of the final connected call.

Optionally, you can also set the `VOICE` environment variable to one of the [voice options](<https://developers.openai.com/api/reference/resources/realtime/subresources/calls/methods/accept#(resource)%20realtime%20%3E%20(model)%20realtime_audio_config%20%3E%20(schema)%20%3E%20(property)%20output%20%2B%20(resource)%20realtime%20%3E%20(model)%20realtime_audio_config_output%20%3E%20(schema)%20%3E%20(property)%20voice>) available.

## PBX Setup

Your SIP PBX will need to be set up to send calls intended for the operator to the URL `sip:$PROJECT_ID@sip.api.openai.com;transport=tls`, where $PROJECT_ID is your OpenAI project ID. It will also need to allow transfers from this endpoint.

Personally, I use and recommend [Asterisk](https://www.asterisk.org/). A minimal configuration for this may look something like:

`extensions.conf`:

```
[LocalSets]

; ... Other extension configurations

exten => Operator,1,Set(__TRANSFER_CONTEXT=LocalSets)
	same => n,Dial(PJSIP/proj_yourprojectid@openai)
```

`pjsip.conf`:

```
[openai]
type=endpoint
context=LocalSets
transport=your-transport
allow=!all,ulaw
outbound_proxy=sip:sip.api.openai.com\;lr
direct_media=no
trust_id_inbound=no
trust_id_outbound=no
context=outbound
rtp_symmetric=yes
media_address=your-external-media-address
bind_rtp_to_media_address=yes
aors=openai

[openai]
type=aor
outbound_proxy=sip:sip.api.openai.com\;lr
contact=sip:sip.api.openai.com
```

And for the true operator experience, you'll probably want to set up any target phones to hotline to the Operator extension.

## Contact Lists

Contact lists can be provided in the form of a `contacts.csv` file in the working directory of the application, with the headers `name` and `number`. eg:

```csv
name,number
Alice,5551234
Bob,5554321
```

Contacts provided in this format will be diallable by name.

## Lifecycle of an Operator phone call.

1. PBX directs the call to the OpenAI SIP endpoint.
2. A webhook is fired to the operator, which directs the call to be answered.
3. The operator provides the initial prompt and opens the conversation.
4. The model queries the user for name or number.
5. Depending on the user response, one of two tool calls are made:
   - to validate digits match a call plan
   - to check against the contact list
6. If the user is not understood, or a tool call validation fails, loop to step 4.
7. If a validation is successful, a SIP Transfer request is made on the original outbound call to the validated location.
