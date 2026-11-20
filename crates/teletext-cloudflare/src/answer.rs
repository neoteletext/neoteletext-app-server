//! One web visitor's wait for what a page's address shows (REQ-1728):
//! the object's request handler awaits it while the object goes on taking
//! its other events, and the runtime's `Effect::Described` ends it, at
//! once or within the runtime's bound, which the object's alarm keeps. A
//! Durable Object runs on one thread, so the two halves share a cell.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use teletext_runtime::descriptions::Answer;

/// What the two halves share: the answer once given, and the waiting
/// task to wake.
#[derive(Default)]
struct Shared {
    answer: Option<Answer>,
    waker: Option<Waker>,
}

/// The half the object keeps until the runtime answers.
pub struct Pending(Rc<RefCell<Shared>>);

/// The half the request handler awaits.
pub struct Waiting(Rc<RefCell<Shared>>);

/// A visitor's wait: the half that answers and the half that waits.
pub fn pair() -> (Pending, Waiting) {
    let shared = Rc::new(RefCell::new(Shared::default()));
    (Pending(Rc::clone(&shared)), Waiting(shared))
}

impl Pending {
    /// Gives the answer, and wakes the handler waiting for it.
    pub fn answer(self, answer: Answer) {
        let waker = {
            let mut shared = self.0.borrow_mut();
            shared.answer = Some(answer);
            shared.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl Drop for Pending {
    /// A wait whose answering half goes unanswered (the object's memory
    /// went with it) ends with no answer: the page is then not public.
    fn drop(&mut self) {
        let waker = {
            let mut shared = self.0.borrow_mut();
            shared.answer.get_or_insert(None);
            shared.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl Future for Waiting {
    type Output = Answer;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Answer> {
        let mut shared = self.0.borrow_mut();
        if let Some(answer) = shared.answer.take() {
            return Poll::Ready(answer);
        }
        shared.waker = Some(context.waker().clone());
        Poll::Pending
    }
}
