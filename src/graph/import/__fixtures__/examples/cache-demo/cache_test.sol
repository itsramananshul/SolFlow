workflow "cache_demo" {
    let n = call("numbers.get", {});
    print("Got number:", n.value);
    let msg = call("printer.print", {"value": n.value});
    print("Printed:", msg.printed);

    let n2 = call("numbers.get", {});
    print("Got number 2:", n2.value);
    let msg2 = call("printer.print", {"value": n2.value});
    print("Printed 2:", msg2.printed);

    print("Cache demo complete");
}
