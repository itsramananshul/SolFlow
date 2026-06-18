workflow greet_user {
    call("greeter.hello", {"name": "OpenPrem"})
}

workflow farewell {
    call("greeter.goodbye", {})
}
