use super::*;

fn scratch(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("teletext-apps-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// A program in `sh` that declares a page, echoes the first line it is
/// sent back as a log line, publishes once, then runs `then`.
fn program(then: &str) -> Vec<String> {
    let script = format!(
        r#"printf '%s\n' '{{"type":"pages","pages":[{{"page":"100","title":"Front"}}]}}'
read line
printf '{{"type":"log","text":"got %s"}}\n' "$(printf '%s' "$line" | tr -d '"{{}}')"
printf '%s\n' '{{"type":"publish","page":"100","owner":"channel","tti":"PN,10000\nOL,1,Hello\n"}}'
{then}"#
    );
    vec!["sh".to_owned(), "-c".to_owned(), script]
}

#[test]
fn the_channels_file_names_each_program() {
    let folder = scratch("file");
    let file = folder.join("channels.json");
    std::fs::write(&file, br#"{"channels":[{"folder":"hackaday","name":"Hackaday","command":["uv","run","main.py"],"directory":"channels/hackaday"},{"folder":"showcase","command":["node","main.ts"]}]}"#).unwrap();
    let specs = read_channels(&file).unwrap();
    assert_eq!(specs.len(), 2);
    assert_eq!(
        specs[0],
        ChannelSpec {
            folder: "hackaday".to_owned(),
            name: "Hackaday".to_owned(),
            command: vec!["uv".to_owned(), "run".to_owned(), "main.py".to_owned()],
            directory: folder.join("channels/hackaday")
        }
    );
    assert_eq!(specs[1].name, "showcase", "the folder names an unnamed channel");
    for bad in [
        &br#"{"channels":[{"folder":"../up","command":["x"]}]}"#[..],
        br#"{"channels":[{"folder":"a","command":[]}]}"#,
        br#"{"channels":[{"folder":"a","command":["x"]},{"folder":"a","command":["y"]}]}"#,
        br#"{"channel":[]}"#,
    ] {
        std::fs::write(&file, bad).unwrap();
        assert!(read_channels(&file).is_err(), "{}", String::from_utf8_lossy(bad));
    }
}

async fn next(link: &mut Link) -> FromApplication {
    tokio::time::timeout(Duration::from_secs(10), link.from.recv()).await.unwrap().unwrap()
}

/// The program declares its pages, hears what the channel sends and is
/// heard; when it ends it is started again, and the channel is told.
#[tokio::test]
async fn a_program_is_started_heard_and_started_again() {
    let spec = ChannelSpec { folder: "test".to_owned(), name: "Test".to_owned(), command: program("exit 0"), directory: scratch("run") };
    let (pages, mut link) = start(spec).await.unwrap();
    assert_eq!(pages.iter().map(|entry| entry.number).collect::<Vec<_>>(), [0x100]);
    link.to.send(b"{\"type\":\"start\"}\n".to_vec()).await.unwrap();
    let FromApplication::Line(echo) = next(&mut link).await else { panic!("a line first") };
    assert_eq!(String::from_utf8(echo).unwrap(), r#"{"type":"log","text":"got type:start"}"#);
    let FromApplication::Line(publish) = next(&mut link).await else { panic!("then the page") };
    assert!(publish.starts_with(br#"{"type":"publish""#));
    // It exits; after the first pause it runs again and declares its pages.
    assert!(matches!(next(&mut link).await, FromApplication::Restarted));
}

/// A program that declares something else first, or nothing, is refused.
#[tokio::test]
async fn a_program_must_declare_its_pages_first() {
    let refuse = |command: &str| ChannelSpec {
        folder: "test".to_owned(),
        name: "Test".to_owned(),
        command: vec!["sh".to_owned(), "-c".to_owned(), command.to_owned()],
        directory: scratch("refuse"),
    };
    let error = start(refuse(r#"printf '%s\n' '{"type":"log","text":"hello"}'"#)).await.unwrap_err();
    assert!(error.contains("the first line must be"), "{error}");
    let error = start(refuse("exit 3")).await.unwrap_err();
    assert!(error.contains("ended before declaring"), "{error}");
    let error = start(ChannelSpec { command: vec!["/no/such/program".to_owned()], ..refuse("") }).await.unwrap_err();
    assert!(error.contains("cannot start"), "{error}");
}
