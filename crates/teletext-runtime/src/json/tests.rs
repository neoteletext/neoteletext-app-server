use super::*;

#[test]
fn a_line_reads_as_a_value() {
    let value = parse(br#"{"type":"publish","page":"1A0","owner":7,"ok":true,"none":null,"list":[1,"two",{"three":3}],"text":"a\u00e9\n\"q\""}"#).unwrap();
    assert_eq!(value.get("type").and_then(Value::text), Some("publish"));
    assert_eq!(value.get("owner").and_then(Value::integer), Some(7));
    assert_eq!(value.get("ok").and_then(Value::boolean), Some(true));
    assert_eq!(value.get("none"), Some(&Value::Null));
    assert_eq!(value.get("list").and_then(Value::items).map(<[Value]>::len), Some(3));
    assert_eq!(value.get("text").and_then(Value::text), Some("a\u{e9}\n\"q\""));
}

#[test]
fn what_is_not_one_integer_value_is_refused() {
    assert_eq!(parse(b"{} {}"), Err(JsonFault::Grammar), "the reader refuses a second value");
    assert_eq!(parse(b""), Err(JsonFault::Grammar), "no value at all");
    assert_eq!(parse(b"1.5"), Err(JsonFault::Number));
    assert_eq!(parse(b"{\"a\":"), Err(JsonFault::Grammar));
    assert_eq!(parse(b"\xff"), Err(JsonFault::Grammar));
    let deep = alloc::format!("{}{}", "[".repeat(40), "]".repeat(40));
    assert_eq!(parse(deep.as_bytes()), Err(JsonFault::Grammar), "nesting is bounded");
}

#[test]
fn a_written_line_reads_back() {
    let mut inner = Writer::nested();
    inner.text("kind", "key").integer("row", -3);
    let mut writer = Writer::new("input");
    writer.text("text", "tab\t\"quote\"\u{1}\u{7f}\u{e9}").boolean("pressed", false).object("event", inner);
    let line = writer.line();
    assert_eq!(line.last(), Some(&b'\n'));
    assert!(!line[..line.len() - 1].contains(&b'\n'), "one line");
    let value = parse(&line[..line.len() - 1]).unwrap();
    assert_eq!(value.get("text").and_then(Value::text), Some("tab\t\"quote\"\u{1}\u{7f}\u{e9}"));
    assert_eq!(value.get("event").and_then(|event| event.get("row")).and_then(Value::integer), Some(-3));
}
