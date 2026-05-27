// Calls an `ext` function that has no endpoint configured. The
// analyzer accepts the call (the function is declared); codegen
// fires E0051 because no endpoint mapping was supplied.
ext function fetch_data(url: str) -> str;

function start() -> str {
    return fetch_data("https://example.com");
}
