use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use gpui::{App, AsyncApp};

use crate::SoukouApp;

#[derive(Clone)]
pub(super) struct OpenUrlListener(UnboundedSender<Vec<String>>);

impl OpenUrlListener {
    pub(super) fn new() -> (Self, UnboundedReceiver<Vec<String>>) {
        let (tx, rx) = futures::channel::mpsc::unbounded::<Vec<String>>();
        (Self(tx), rx)
    }

    pub(super) fn open(&self, urls: Vec<String>) {
        if let Err(error) = self.0.unbounded_send(urls) {
            eprintln!("failed to receive auth callback url: {error}");
        }
    }
}

pub(super) fn spawn_open_url_handler(
    cx: &mut App,
    main_window: gpui::WindowHandle<SoukouApp>,
    mut open_url_receiver: UnboundedReceiver<Vec<String>>,
) {
    cx.spawn(move |cx: &mut AsyncApp| {
        let mut app = cx.clone();
        async move {
            while let Some(urls) = open_url_receiver.next().await {
                if let Err(error) = main_window.update(&mut app, |this, _, cx| {
                    this.handle_open_urls(urls, cx);
                }) {
                    eprintln!(
                        "failed to handle auth callback url:
                      {error}"
                    );
                }
            }
        }
    })
    .detach();
}
