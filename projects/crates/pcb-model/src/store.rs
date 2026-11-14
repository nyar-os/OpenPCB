//! Process-wide handle store.
//!
//! The Node-API surface, the command line and the studio all address a board by
//! a numeric handle. Keeping the store here means those entry points cannot
//! drift into separate board models. Each handle also owns a bounded undo and
//! redo history, so every surface restores the same snapshots.

use std::{
    collections::BTreeMap,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use boost_session::History;
use serde_json::Value;

use crate::{
    ServiceError,
    board::{Board, summary},
};

/// Snapshots retained per handle in each direction.
const HISTORY_LIMIT: usize = 32;

/// One open board plus its bounded edit history.
struct Entry {
    board: Board,
    history: History<Board>,
}

static STORE: OnceLock<Mutex<BTreeMap<u64, Entry>>> = OnceLock::new();
static HANDLE_SEQ: AtomicU64 = AtomicU64::new(0);
static BOARD_SEQ: AtomicU64 = AtomicU64::new(0);

fn store() -> &'static Mutex<BTreeMap<u64, Entry>> {
    STORE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn lock() -> std::sync::MutexGuard<'static, BTreeMap<u64, Entry>> {
    // A poisoned lock still owns a coherent map; recovering keeps every open
    // board reachable after one panicking call.
    store().lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Allocates the next deterministic board id.
pub(crate) fn next_board_id() -> String {
    let sequence = BOARD_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    format!("board-{sequence}")
}

/// Inserts an open board and returns its handle. The history starts empty.
pub(crate) fn insert(board: Board) -> u64 {
    let handle = HANDLE_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    lock().insert(handle, Entry { board, history: History::new(HISTORY_LIMIT) });
    handle
}

/// Runs a read-only operation on one open board.
pub(crate) fn read<R>(handle: u64, operation: impl FnOnce(&Board) -> R) -> Result<R, ServiceError> {
    let boards = lock();
    let entry = boards.get(&handle).ok_or_else(|| not_found(handle))?;
    Ok(operation(&entry.board))
}

/// Runs a mutating operation and returns the committed summary.
///
/// The operation runs on a clone and only replaces the board when it succeeds,
/// so a rejected edit leaves both the board and the history untouched. A
/// successful edit pushes the previous snapshot and clears the redo stack.
pub(crate) fn mutate(handle: u64, operation: impl FnOnce(&mut Board) -> Result<(), ServiceError>) -> Result<(), ServiceError> {
    mutate_if_changed(handle, |board| {
        operation(board)?;
        Ok(true)
    })
}

/// Runs a mutating operation that reports whether it changed the board.
///
/// An operation that returns `false` discards its clone, leaving the board and
/// its history untouched. This lets bulk edits such as the router avoid empty
/// undo steps when they accept nothing.
pub(crate) fn mutate_if_changed(
    handle: u64,
    operation: impl FnOnce(&mut Board) -> Result<bool, ServiceError>,
) -> Result<(), ServiceError> {
    let mut boards = lock();
    let entry = boards.get_mut(&handle).ok_or_else(|| not_found(handle))?;
    let mut next = entry.board.clone();
    if !operation(&mut next)? {
        return Ok(());
    }
    let previous = std::mem::replace(&mut entry.board, next);
    entry.history.record(previous);
    Ok(())
}

/// Restores the previous snapshot and bumps the revision.
pub(crate) fn undo(handle: u64) -> Result<(), ServiceError> {
    let mut boards = lock();
    let entry = boards.get_mut(&handle).ok_or_else(|| not_found(handle))?;
    let current = entry.board.clone();
    let Some(mut snapshot) = entry.history.undo(current)
    else {
        return Err(ServiceError::new("board/history-empty", "there is no earlier revision to restore"));
    };
    snapshot.revision = entry.board.revision + 1;
    entry.board = snapshot;
    Ok(())
}

/// Reapplies the next snapshot and bumps the revision.
pub(crate) fn redo(handle: u64) -> Result<(), ServiceError> {
    let mut boards = lock();
    let entry = boards.get_mut(&handle).ok_or_else(|| not_found(handle))?;
    let current = entry.board.clone();
    let Some(mut snapshot) = entry.history.redo(current)
    else {
        return Err(ServiceError::new("board/history-empty", "there is no later revision to restore"));
    };
    snapshot.revision = entry.board.revision + 1;
    entry.board = snapshot;
    Ok(())
}

/// Clones the current summary of one open board, including history flags.
pub(crate) fn summary_of(handle: u64) -> Result<Value, ServiceError> {
    let boards = lock();
    let entry = boards.get(&handle).ok_or_else(|| not_found(handle))?;
    let mut value = summary(&entry.board);
    value["canUndo"] = Value::Bool(entry.history.can_undo());
    value["canRedo"] = Value::Bool(entry.history.can_redo());
    Ok(value)
}

/// Removes one open board.
pub(crate) fn remove(handle: u64) -> Result<(), ServiceError> {
    lock().remove(&handle).map(|_| ()).ok_or_else(|| not_found(handle))
}

fn not_found(handle: u64) -> ServiceError {
    ServiceError::new("board/not-found", format!("no open board has handle {handle}"))
}
