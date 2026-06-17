import "send" from slack;

workflow "notify" {
    slack.send({ msg: "hi" });
    call("alert.fire", { level: 3 });
    emit "done";
}
