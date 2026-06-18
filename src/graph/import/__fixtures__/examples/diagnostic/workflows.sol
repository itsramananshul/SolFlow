import system;

workflow "collect_all" {
    let data = system.diagnostics({});
    print("Hostname:", data.hostname.hostname);
    print("Platform:", data.hostname.platform);
    print("Uptime:", data.uptime.uptime_str);
    print("CPU:", data.cpu.total_percent, "%");
    print("Memory:", data.memory.percent, "%");
    print("Disk:", data.disk.partitions[0].percent, "%");
    print("Processes:", data.processes.total);
    let top = data.processes.processes[0];
    print("Top:", top.name, top.cpu_percent, "%");
    print("Done");
}

workflow "cpu_health" {
    let data = system.cpu({});
    print("Cores:", data.count);
    print("Load:", data.total_percent, "%");
    print("Done");
}

workflow "storage_check" {
    let mem = system.memory({});
    let disk = system.disk({});
    print("Memory:", mem.percent, "%");
    print("Disk count:", len(disk.partitions));
    print("Done");
}

workflow "top_procs" {
    let data = system.processes({});
    print("Processes:", data.total);
    let top = data.processes[0];
    print("Top:", top.name, top.cpu_percent, "%");
    print("Done");
}
