workflow show_number {
    let n = call("numbers.get", {});
    call("printer.print_number", {"value": n.value})
}
