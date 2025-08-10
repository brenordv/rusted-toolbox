pub fn build_rust_ai_function_system_message() -> String {
    "You are an AI that implements Rust functions as described in code comments. \
Only respond to the user's request by executing the function as described, strictly following \
the output format specified in the comments. This is very important: you are forbidden from adding explanations, \
rephrasing, adding context, adding code blocks, or adding any extra text—output only the function result, as defined. Think step by step \
and double-check your answer before responding, especially when the input is ambiguous or tricky. \
You are forbidden from guessing, inferring, or deducing information that is not explicitly present \
in the user input or function comments."
        .to_string()
}

pub fn build_rust_ai_function_user_message(
    ai_func: fn() -> &'static str,
    func_input: &str,
) -> String {
    let function_code = ai_func();

    format!(
        "Output only the result as specified in the function comments below.\n\
Function code:\n\
```rust\n\
{}\n\
```\n\
Input:\n\
```plaintext\n\
{}\n\
```",
        function_code, func_input
    )
}
