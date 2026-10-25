use super::*;
use teletext_runtime::remote::declared_pages;
use teletext_runtime::{ChannelConfig, Remote};

/// A page the program retitles is listed by its new title in the
/// contents the host serves, without the channel starting again.
#[tokio::test]
async fn a_retitled_page_is_served_in_the_contents() {
    let pages = declared_pages(br#"{"type":"pages","pages":[{"page":"100","title":"Front"},{"page":"200","title":"Story 1"}]}"#).unwrap();
    let config = ChannelConfig { name: "test".to_owned(), push_address: "ws://127.0.0.1:1/test/push".to_owned(), initial_page: 0x100, epoch_at_start_ms: 0 };
    let channel = Channel::new(config, vec![Box::new(Remote::new(pages))], 1);
    let (lines, from) = mpsc::channel(1);
    let (to, _heard) = mpsc::channel(16);
    let hub = spawn("test", channel, Instant::now(), Some(Link { from, to }), Arc::new(ServerStats::default()));
    let served = || String::from_utf8(hub.contents.read().unwrap().to_vec()).unwrap();
    assert!(served().contains("p\t200\tStory 1\n"));
    lines.send(FromApplication::Line(br#"{"type":"title","page":"200","title":"A robot arm"}"#.to_vec())).await.unwrap();
    let retitled = tokio::time::timeout(Duration::from_secs(10), async {
        while !served().contains("p\t200\tA robot arm\n") {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(retitled.is_ok(), "{}", served());
}
