import finance;
import visualization;

workflow "viz_session" {
    let raw = finance.get_data({});
    let result = visualization.graph({ data: raw });
    result;
}
