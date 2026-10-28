use super::*;
use teletext_runtime::remote::declared_pages;
use teletext_runtime::{ChannelConfig, Remote, StoreKey};

const FRONT: &[u8] = br#"{"type":"pages","pages":[{"page":"100","title":"Front"},{"page":"200","title":"Story 1"}]}"#;

fn config() -> ChannelConfig {
    ChannelConfig {
        name: "test".to_owned(),
        push_address: "ws://127.0.0.1:1/test/push".to_owned(),
        initial_page: 0x100,
        epoch_at_start_ms: 0,
        session_grace_ms: 0,
    }
}

/// Waits until `done` holds, up to ten seconds.
async fn until(mut done: impl FnMut() -> bool) -> bool {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !done() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .is_ok()
}

/// A page the program retitles is listed by its new title in the
/// contents the host serves, without the channel starting again.
#[tokio::test]
async fn a_retitled_page_is_served_in_the_contents() {
    let channel = Channel::new(config(), vec![Box::new(Remote::new(declared_pages(FRONT).unwrap()))], 1);
    let (lines, from) = mpsc::channel(1);
    let (to, _heard) = mpsc::channel(16);
    let hub = spawn("test", channel, Instant::now(), Some(Link { from, to }), Arc::new(ServerStats::default()), None);
    let served = || String::from_utf8(hub.contents.read().unwrap().to_vec()).unwrap();
    lines.send(FromApplication::Line(br#"{"type":"publish","page":"200","owner":"channel","tti":"PN,20000\nOL,1,A robot arm\n"}"#.to_vec())).await.unwrap();
    assert!(until(|| served().contains("p\t200\tStory 1\n")).await, "listed once published: {}", served());
    lines.send(FromApplication::Line(br#"{"type":"title","page":"200","title":"A robot arm"}"#.to_vec())).await.unwrap();
    assert!(until(|| served().contains("p\t200\tA robot arm\n")).await, "{}", served());
}

/// No line reaches a program before it declares its pages, nor after it
/// ends; once it declares, it is sent `start` first (requirement 5.3).
#[tokio::test]
async fn lines_reach_a_program_only_while_it_runs() {
    let channel = Channel::new(config(), vec![Box::new(Remote::new(Vec::new()))], 1);
    let (lines, from) = mpsc::channel(4);
    let (to, mut heard) = mpsc::channel(16);
    let hub = spawn("test", channel, Instant::now(), Some(Link { from, to }), Arc::new(ServerStats::default()), None);
    let (outbox, _own) = mpsc::channel(OUTBOX);
    hub.commands.send(Command::Opened { session: 7, outbox }).await.unwrap();
    lines.send(FromApplication::Declared(declared_pages(FRONT).unwrap())).await.unwrap();
    let first = tokio::time::timeout(Duration::from_secs(10), heard.recv()).await.unwrap().unwrap();
    assert!(first.starts_with(br#"{"type":"start""#), "start first: {}", String::from_utf8_lossy(&first));
    let second = tokio::time::timeout(Duration::from_secs(10), heard.recv()).await.unwrap().unwrap();
    assert_eq!(second, b"{\"type\":\"join\",\"session\":7}\n", "then the viewer already here");
    lines.send(FromApplication::Ended).await.unwrap();
    // The program's lines and the connections' commands are two queues:
    // the end must be taken before the next viewer comes.
    assert!(until(|| lines.capacity() == lines.max_capacity()).await);
    let (outbox, _own) = mpsc::channel(OUTBOX);
    hub.commands.send(Command::Opened { session: 8, outbox }).await.unwrap();
    let (done, stopped) = oneshot::channel();
    hub.commands.send(Command::Shutdown { done }).await.unwrap();
    stopped.await.unwrap();
    assert!(heard.try_recv().is_err(), "nothing for a program that ended");
}

/// A stopping channel closes its connections as a restart, and writes what
/// it keeps before it answers (requirements 5.1 and 5.2).
#[tokio::test]
async fn a_stopping_channel_closes_its_connections_and_keeps_its_pages() {
    let root = std::env::temp_dir().join(format!("teletext-hub-keep-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let state = StateDirectory::open(&root).unwrap();
    let channel = Channel::new(config(), vec![Box::new(Remote::new(Vec::new()))], 1);
    let (lines, from) = mpsc::channel(4);
    let (to, _heard) = mpsc::channel(16);
    let hub = spawn("test", channel, Instant::now(), Some(Link { from, to }), Arc::new(ServerStats::default()), Some(state.clone()));
    let (outbox, mut own) = mpsc::channel(OUTBOX);
    hub.commands.send(Command::Opened { session: 7, outbox }).await.unwrap();
    lines.send(FromApplication::Declared(declared_pages(FRONT).unwrap())).await.unwrap();
    lines.send(FromApplication::Line(br#"{"type":"publish","page":"100","owner":"channel","tti":"PN,10000\nOL,1,Kept\n"}"#.to_vec())).await.unwrap();
    assert!(until(|| hub.pages.frame(0x100).is_some()).await);
    let (done, stopped) = oneshot::channel();
    hub.commands.send(Command::Shutdown { done }).await.unwrap();
    stopped.await.unwrap();
    let mut closed = None;
    while let Ok(outbound) = own.try_recv() {
        if let Outbound::Close(code) = outbound {
            closed = Some(code);
        }
    }
    assert_eq!(closed, Some(CLOSE_SERVICE_RESTART));
    let kept: Vec<StoreKey> = state.records("test").0.into_iter().map(|(key, _)| key).collect();
    assert_eq!(kept, [StoreKey::Page(0x100), StoreKey::Pages(0)]);
    let _ = std::fs::remove_dir_all(&root);
}
