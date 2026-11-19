//! A channel's connections, with the time each next needs the channel's
//! deadline kept in order (TODO-6895), so that the next wake and the
//! connections due are found without looking at every connection.
//!
//! A host asks [`super::Channel::next_wake`] after every event it gives the
//! channel. Counted from every connection, that cost the channel's whole
//! count of connections on every message a viewer or the program sent,
//! five times over: measured on 19 November 2026, the channel's task was a
//! whole processor core busy opening 10,000 connections at 2,000 a second,
//! and the program's answers to them came too late. Here each connection's
//! wake is counted again only when it may have changed, which is when the
//! connection was lent out to be changed.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use super::Connection;
use crate::protocol::ConnectionId;

/// The connections, each with its next wake. Its methods take a
/// connection's number by reference, as the map it stands in for does, so
/// that the channel's every use of the map reads as before.
#[derive(Default)]
pub(super) struct ConnectionTable {
    map: BTreeMap<ConnectionId, Connection>,
    /// The wake each connection had when it was last counted.
    wakes: BTreeMap<ConnectionId, u64>,
    /// The same, in order of time.
    order: BTreeSet<(u64, ConnectionId)>,
    /// The connections lent out to be changed since they were last counted.
    changed: BTreeSet<ConnectionId>,
}

#[expect(clippy::trivially_copy_pass_by_ref, reason = "the same signatures as the BTreeMap the table stands in for, used at a hundred places")]
impl ConnectionTable {
    pub(super) fn get(&self, connection: &ConnectionId) -> Option<&Connection> {
        self.map.get(connection)
    }

    /// The connection, to change: its wake is counted again before it is
    /// next asked for.
    pub(super) fn get_mut(&mut self, connection: &ConnectionId) -> Option<&mut Connection> {
        let state = self.map.get_mut(connection)?;
        self.changed.insert(*connection);
        Some(state)
    }

    pub(super) fn contains_key(&self, connection: &ConnectionId) -> bool {
        self.map.contains_key(connection)
    }

    pub(super) fn keys(&self) -> impl Iterator<Item = &ConnectionId> {
        self.map.keys()
    }

    pub(super) fn values(&self) -> impl Iterator<Item = &Connection> {
        self.map.values()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&ConnectionId, &Connection)> {
        self.map.iter()
    }

    /// Every connection, to change: each one's wake is counted again.
    pub(super) fn iter_mut(&mut self) -> impl Iterator<Item = (&ConnectionId, &mut Connection)> {
        self.changed.extend(self.map.keys().copied());
        self.map.iter_mut()
    }

    /// Every connection, to change, as [`ConnectionTable::iter_mut`].
    pub(super) fn values_mut(&mut self) -> impl Iterator<Item = &mut Connection> {
        self.iter_mut().map(|(_, state)| state)
    }

    pub(super) fn insert(&mut self, connection: ConnectionId, state: Connection) -> Option<Connection> {
        self.changed.insert(connection);
        self.map.insert(connection, state)
    }

    pub(super) fn remove(&mut self, connection: &ConnectionId) -> Option<Connection> {
        self.forget(*connection);
        self.changed.remove(connection);
        self.map.remove(connection)
    }

    /// Takes `connection`'s wake out of the order.
    fn forget(&mut self, connection: ConnectionId) {
        if let Some(wake) = self.wakes.remove(&connection) {
            self.order.remove(&(wake, connection));
        }
    }

    /// Counts again the wake of every connection changed since.
    fn settle(&mut self) {
        for connection in core::mem::take(&mut self.changed) {
            self.forget(connection);
            if let Some(wake) = self.map.get(&connection).and_then(Connection::wake) {
                self.wakes.insert(connection, wake);
                self.order.insert((wake, connection));
            }
        }
    }

    /// The earliest of the connections' wakes.
    pub(super) fn next_wake(&mut self) -> Option<u64> {
        self.settle();
        self.order.first().map(|&(wake, _)| wake)
    }

    /// The connections whose wake is `now` or earlier: those any of the
    /// deadline's work may be due for.
    pub(super) fn due(&mut self, now: u64) -> Vec<ConnectionId> {
        self.settle();
        self.order.range(..=(now, ConnectionId::MAX)).map(|&(_, connection)| connection).collect()
    }

    /// Whether the order holds every connection's wake as it is now, for
    /// the tests.
    #[cfg(test)]
    pub(super) fn settled_wakes_are_current(&mut self) -> bool {
        self.settle();
        self.map.iter().all(|(id, state)| self.wakes.get(id).copied() == state.wake())
    }
}

impl<'table> IntoIterator for &'table ConnectionTable {
    type Item = (&'table ConnectionId, &'table Connection);
    type IntoIter = alloc::collections::btree_map::Iter<'table, ConnectionId, Connection>;
    fn into_iter(self) -> Self::IntoIter {
        self.map.iter()
    }
}
