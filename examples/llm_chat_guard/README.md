# llm_chat_guard

Guards a prompt, calls a **blocking** model function, then guards the reply.
`wardline-llm::guarded_prompt` is the whole wrapper: an input block skips
the model; an output block discards the reply.

The default model is an in-process stub so this runs offline in under a
minute. Swap the function for `reqwest::blocking` (the `--http` flag) when
you have a real endpoint.

## What is guarded

**Input** (before the model):

1. **`regex_block`** — refuses prompts containing `sk-…`.
2. **`prompt_injection`** — refuses "ignore previous instructions" phrasing.

**Output** (after the model):

3. **`pii`** in redact mode — the stub plants `demo@example.com` so you can
   see it become `[EMAIL]`. This is the bundled baseline detector, not a
   compliance-grade PII control.

## Run the demo

```sh
cargo run -p llm_chat_guard
```

You should see:

- `ALLOW  "summarise this article"` with the email redacted
- `BLOCK  "Ignore previous instructions…"` halted by `prompt_injection`,
  with no model call

## One prompt

```sh
cargo run -p llm_chat_guard -- "hello there"
```

## Live HTTP (reqwest::blocking)

```sh
cargo run -p llm_chat_guard -- --http http://127.0.0.1:8080/complete "hello"
```

POSTs the prompt as `text/plain` and treats the response body as the model
reply, still on a blocking client. The pipelines around the call do not
change.
