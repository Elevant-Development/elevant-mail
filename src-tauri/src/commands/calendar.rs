//! Calendar: calendars and their events from the cache, syncing them over CalDAV, and answering
//! invitations that came by mail.

use elevant_mail_core::calendar::events::{
    self, Calendar, EventDetails, EventInput, Occurrence, Reminder,
};
use elevant_mail_core::calendar::invitation::{self, InvitationAnswer};
use elevant_mail_core::calendar::sync::{self, CaldavStatus, CalendarSyncReport};
use tauri::State;

use crate::error::CommandError;
use crate::state::AppState;

#[tauri::command]
pub async fn list_calendars(state: State<'_, AppState>) -> Result<Vec<Calendar>, CommandError> {
    Ok(events::list_calendars(&state.db.conn())?)
}

#[tauri::command]
pub async fn set_calendar_visible(
    state: State<'_, AppState>,
    calendar_id: String,
    visible: bool,
) -> Result<(), CommandError> {
    Ok(events::set_calendar_visible(
        &state.db.conn(),
        &calendar_id,
        visible,
    )?)
}

/// Gives a calendar a colour of its own; `None` shows it in its account's colour again.
#[tauri::command]
pub async fn set_calendar_color(
    state: State<'_, AppState>,
    calendar_id: String,
    color: Option<String>,
) -> Result<(), CommandError> {
    Ok(events::set_calendar_color(
        &state.db.conn(),
        &calendar_id,
        color.as_deref(),
    )?)
}

/// The dates of all events that overlap `from..to` (Unix milliseconds), series expanded.
#[tauri::command]
pub async fn list_occurrences(
    state: State<'_, AppState>,
    from: i64,
    to: i64,
) -> Result<Vec<Occurrence>, CommandError> {
    Ok(events::occurrences(&state.db.conn(), from, to)?)
}

/// The reminders due in `from..=to` (Unix milliseconds), for the interface to show.
#[tauri::command]
pub async fn due_reminders(
    state: State<'_, AppState>,
    from: i64,
    to: i64,
) -> Result<Vec<Reminder>, CommandError> {
    Ok(events::due_reminders(&state.db.conn(), from, to)?)
}

#[tauri::command]
pub async fn get_event(
    state: State<'_, AppState>,
    event_id: String,
    instance: Option<i64>,
) -> Result<EventDetails, CommandError> {
    Ok(events::get_event_at(&state.db.conn(), &event_id, instance)?)
}

/// A new event or the changes to one; returns its id.
#[tauri::command]
pub async fn save_event(
    state: State<'_, AppState>,
    event: EventInput,
) -> Result<String, CommandError> {
    Ok(events::save_event(&state.db.conn(), &event)?)
}

/// A series goes with all its dates.
#[tauri::command]
pub async fn delete_event(
    state: State<'_, AppState>,
    event_id: String,
    instance: Option<i64>,
) -> Result<(), CommandError> {
    Ok(events::delete_event_at(
        &state.db.conn(),
        &event_id,
        instance,
    )?)
}

/// Sends the answer to the organizer and returns once the SMTP server took it.
#[tauri::command]
pub async fn answer_invitation(
    state: State<'_, AppState>,
    answer: InvitationAnswer,
) -> Result<(), CommandError> {
    invitation::answer(&state.db, state.secrets.as_ref(), &answer).await?;
    Ok(())
}

/// Sends the changes made here and takes the server's; one at a time per account.
#[tauri::command]
pub async fn sync_calendars(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<CalendarSyncReport, CommandError> {
    let lock = state.account_lock(&format!("calendar:{account_id}"));
    let _turn = lock.lock().await;
    sync::sync_account(&state.db, state.secrets.as_ref(), &account_id)
        .await
        .map_err(|error| {
            tracing::warn!(account = %account_id, %error, "calendar sync failed");
            error.into()
        })
}

#[tauri::command]
pub async fn caldav_status(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<CaldavStatus, CommandError> {
    Ok(sync::status(&state.db.conn(), &account_id)?)
}

/// Switches the account's calendars on or off; a server address is checked before it is kept.
#[tauri::command]
pub async fn set_caldav(
    state: State<'_, AppState>,
    account_id: String,
    enabled: bool,
    url: Option<String>,
) -> Result<CaldavStatus, CommandError> {
    let lock = state.account_lock(&format!("calendar:{account_id}"));
    let _turn = lock.lock().await;
    Ok(sync::set_caldav(
        &state.db,
        state.secrets.as_ref(),
        &account_id,
        enabled,
        url.as_deref(),
    )
    .await?)
}
