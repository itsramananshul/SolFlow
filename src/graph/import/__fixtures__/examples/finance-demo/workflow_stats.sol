import finance;
import statistics;

workflow "stats_session" {
    let raw = finance.get_data({});
    let result = statistics.summarize({ data: raw });
    result;
}
