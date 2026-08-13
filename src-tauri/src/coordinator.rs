use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::json;

use crate::domain::{
    Assignment, PersistedState, Sound, parse_cell_id, validate_cell_in_grid, validate_grid,
};
use crate::dto::{
    AppSnapshot, AppWarningDto, CellDto, PlayResult, ProblemDto, ShortcutDto, ShortcutInput,
    ShortcutStatus, SoundDto, Trigger, empty_snapshot,
};
use crate::error::ApiError;
use crate::hotkeys::normalize::normalize_shortcut;
use crate::hotkeys::{HotkeyId, HotkeyService, HotkeyTarget};
use crate::import::{prepare_import, rollback_import};
use crate::ports::{AudioService, FilePicker, PlaybackRequest, RepositoryLoad, StateRepository};

#[derive(Debug, Clone)]
struct SoundRuntime {
    playable: bool,
    problem: Option<ProblemDto>,
    shortcut_status: Option<ShortcutStatus>,
    hotkey_id: Option<HotkeyId>,
}

#[derive(Default)]
struct RuntimeState {
    sounds: HashMap<String, SoundRuntime>,
    startup_warnings: Vec<AppWarningDto>,
    capture_active: bool,
}

pub struct Coordinator {
    state: Mutex<PersistedState>,
    runtime: Mutex<RuntimeState>,
    mutation_gate: Mutex<()>,
    startup_error: Option<ApiError>,
    repository: Arc<dyn StateRepository>,
    audio: Arc<dyn AudioService>,
    hotkeys: Arc<dyn HotkeyService>,
    picker: Arc<dyn FilePicker>,
}

impl Coordinator {
    pub fn initialize(
        load: RepositoryLoad,
        repository: Arc<dyn StateRepository>,
        audio: Arc<dyn AudioService>,
        hotkeys: Arc<dyn HotkeyService>,
        picker: Arc<dyn FilePicker>,
    ) -> Arc<Self> {
        let coordinator = Arc::new(Self {
            state: Mutex::new(load.state),
            runtime: Mutex::new(RuntimeState {
                sounds: HashMap::new(),
                startup_warnings: load.warnings,
                capture_active: false,
            }),
            mutation_gate: Mutex::new(()),
            startup_error: None,
            repository,
            audio,
            hotkeys,
            picker,
        });
        coordinator.initialize_runtime();
        coordinator
    }

    pub fn blocked(
        error: ApiError,
        repository: Arc<dyn StateRepository>,
        audio: Arc<dyn AudioService>,
        hotkeys: Arc<dyn HotkeyService>,
        picker: Arc<dyn FilePicker>,
    ) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(PersistedState::default()),
            runtime: Mutex::new(RuntimeState::default()),
            mutation_gate: Mutex::new(()),
            startup_error: Some(error),
            repository,
            audio,
            hotkeys,
            picker,
        })
    }

    fn initialize_runtime(&self) {
        let state = lock(&self.state).clone();
        let mut runtime = lock(&self.runtime);
        for assignment in &state.assignments {
            let sound_id = assignment.sound.id.to_string();
            let path = self
                .repository
                .audio_path(&assignment.sound.stored_file_name);
            let loaded = path.and_then(|path| self.audio.load(&sound_id, &path));
            let (playable, problem) = match loaded {
                Ok(_) if self.audio.is_available() => (true, None),
                Ok(_) => (
                    false,
                    Some(ProblemDto {
                        code: "AUDIO_DEVICE_UNAVAILABLE".to_owned(),
                        message: "The default audio output device is unavailable.".to_owned(),
                    }),
                ),
                Err(error) => (
                    false,
                    Some(ProblemDto {
                        code: error.code,
                        message: "This managed audio file is missing or could not be decoded."
                            .to_owned(),
                    }),
                ),
            };

            let (shortcut_status, hotkey_id) = match &assignment.sound.shortcut {
                None => (None, None),
                Some(shortcut) => match self.hotkeys.register(shortcut) {
                    Ok(id) => {
                        self.hotkeys.activate(
                            id,
                            HotkeyTarget {
                                request: playback_request(assignment, Trigger::GlobalShortcut),
                            },
                        );
                        (Some(ShortcutStatus::Registered), Some(id))
                    }
                    Err(_) => (Some(ShortcutStatus::Unavailable), None),
                },
            };

            runtime.sounds.insert(
                sound_id,
                SoundRuntime {
                    playable,
                    problem,
                    shortcut_status,
                    hotkey_id,
                },
            );
        }
    }

    fn ensure_available(&self) -> Result<(), ApiError> {
        self.startup_error.clone().map_or(Ok(()), Err)
    }

    pub fn get_state(&self) -> Result<AppSnapshot, ApiError> {
        self.ensure_available()?;
        Ok(self.snapshot())
    }

    pub fn set_shortcut_capture_active(&self, active: bool) -> Result<(), ApiError> {
        self.ensure_available()?;
        let _gate = lock(&self.mutation_gate);
        if lock(&self.runtime).capture_active == active {
            self.hotkeys.set_capture_active(active);
            return Ok(());
        }

        if active {
            self.suspend_shortcuts_for_capture()?;
        } else {
            self.restore_shortcuts_after_capture();
        }
        Ok(())
    }

    fn suspend_shortcuts_for_capture(&self) -> Result<(), ApiError> {
        self.hotkeys.set_capture_active(true);
        let state = lock(&self.state).clone();
        let registered = state
            .assignments
            .iter()
            .filter_map(|assignment| {
                let id = lock(&self.runtime)
                    .sounds
                    .get(&assignment.sound.id.to_string())
                    .and_then(|runtime| runtime.hotkey_id)?;
                assignment.sound.shortcut.as_ref()?;
                Some((assignment.clone(), id))
            })
            .collect::<Vec<_>>();
        let mut suspended = Vec::with_capacity(registered.len());

        for (assignment, old_id) in registered {
            let shortcut = assignment
                .sound
                .shortcut
                .as_ref()
                .expect("registered shortcut exists");
            if let Err(error) = self.hotkeys.unregister(shortcut) {
                for suspended_assignment in suspended {
                    self.restore_one_shortcut(&suspended_assignment);
                }
                self.hotkeys.set_capture_active(false);
                return Err(error);
            }
            self.hotkeys.deactivate(old_id);
            if let Some(runtime) = lock(&self.runtime)
                .sounds
                .get_mut(&assignment.sound.id.to_string())
            {
                runtime.hotkey_id = None;
            }
            suspended.push(assignment);
        }
        lock(&self.runtime).capture_active = true;
        Ok(())
    }

    fn restore_shortcuts_after_capture(&self) {
        let state = lock(&self.state).clone();
        for assignment in &state.assignments {
            if assignment.sound.shortcut.is_none() {
                continue;
            }
            let already_registered = lock(&self.runtime)
                .sounds
                .get(&assignment.sound.id.to_string())
                .is_some_and(|runtime| runtime.hotkey_id.is_some());
            if !already_registered {
                self.restore_one_shortcut(assignment);
            }
        }
        lock(&self.runtime).capture_active = false;
        self.hotkeys.set_capture_active(false);
    }

    fn restore_one_shortcut(&self, assignment: &Assignment) {
        let Some(shortcut) = assignment.sound.shortcut.as_ref() else {
            return;
        };
        let registration = self.hotkeys.register(shortcut);
        let mut runtime = lock(&self.runtime);
        let Some(sound_runtime) = runtime.sounds.get_mut(&assignment.sound.id.to_string()) else {
            return;
        };
        match registration {
            Ok(id) => {
                sound_runtime.shortcut_status = Some(ShortcutStatus::Registered);
                sound_runtime.hotkey_id = Some(id);
                drop(runtime);
                self.hotkeys.activate(
                    id,
                    HotkeyTarget {
                        request: playback_request(assignment, Trigger::GlobalShortcut),
                    },
                );
            }
            Err(_) => {
                sound_runtime.shortcut_status = Some(ShortcutStatus::Unavailable);
                sound_runtime.hotkey_id = None;
            }
        }
    }

    pub fn pick_and_import_sound(&self, cell_id: String) -> Result<Option<AppSnapshot>, ApiError> {
        self.ensure_available()?;
        {
            let state = lock(&self.state);
            validate_cell_in_grid(&cell_id, &state.grid)?;
            if state.assignment(&cell_id).is_some() {
                return Err(ApiError::new(
                    "CELL_OCCUPIED",
                    "That cell already contains a sound.",
                ));
            }
        }
        let Some(source) = self.picker.pick_audio_file()? else {
            return Ok(None);
        };
        let _gate = lock(&self.mutation_gate);
        {
            let state = lock(&self.state);
            validate_cell_in_grid(&cell_id, &state.grid)?;
            if state.assignment(&cell_id).is_some() {
                return Err(ApiError::new(
                    "CELL_OCCUPIED",
                    "Another sound was added to that cell before this import finished.",
                ));
            }
        }

        let prepared = prepare_import(&source, self.repository.audio_dir(), self.audio.as_ref())?;
        let assignment = Assignment {
            cell_id: cell_id.clone(),
            sound: Sound {
                id: prepared.id,
                display_name: prepared.display_name.clone(),
                original_file_name: prepared.original_file_name.clone(),
                stored_file_name: prepared.stored_file_name.clone(),
                format: prepared.format,
                duration_ms: prepared.duration_ms,
                shortcut: None,
            },
        };

        let mut next = lock(&self.state).clone();
        if next.assignment(&cell_id).is_some() {
            rollback_import(&prepared, self.audio.as_ref());
            return Err(ApiError::new(
                "CELL_OCCUPIED",
                "Another sound was added to that cell before this import finished.",
            ));
        }
        next.assignments.push(assignment);
        next.sort_assignments();
        if let Err(error) = self.repository.save(&next) {
            rollback_import(&prepared, self.audio.as_ref());
            return Err(error);
        }

        *lock(&self.state) = next;
        lock(&self.runtime).sounds.insert(
            prepared.id.to_string(),
            runtime_for_loaded_sound(self.audio.is_available(), None, None),
        );
        Ok(Some(self.snapshot()))
    }

    pub fn pick_and_replace_sound(&self, cell_id: String) -> Result<Option<AppSnapshot>, ApiError> {
        self.ensure_available()?;
        {
            let state = lock(&self.state);
            validate_cell_in_grid(&cell_id, &state.grid)?;
            if state.assignment(&cell_id).is_none() {
                return Err(ApiError::new(
                    "CELL_EMPTY",
                    "That cell does not contain a sound.",
                ));
            }
        }
        let Some(source) = self.picker.pick_audio_file()? else {
            return Ok(None);
        };
        let _gate = lock(&self.mutation_gate);
        let old_assignment = {
            let state = lock(&self.state);
            state.assignment(&cell_id).cloned().ok_or_else(|| {
                ApiError::new("CELL_EMPTY", "That cell no longer contains a sound.")
            })?
        };
        let prepared = prepare_import(&source, self.repository.audio_dir(), self.audio.as_ref())?;

        let mut next = lock(&self.state).clone();
        let current = next
            .assignment_mut(&cell_id)
            .ok_or_else(|| ApiError::new("CELL_EMPTY", "That cell no longer contains a sound."));
        let current = match current {
            Ok(current) if current.sound.id == old_assignment.sound.id => current,
            _ => {
                rollback_import(&prepared, self.audio.as_ref());
                return Err(ApiError::new(
                    "NOT_FOUND",
                    "That sound changed before replacement finished.",
                ));
            }
        };
        current.sound = Sound {
            id: prepared.id,
            display_name: prepared.display_name.clone(),
            original_file_name: prepared.original_file_name.clone(),
            stored_file_name: prepared.stored_file_name.clone(),
            format: prepared.format,
            duration_ms: prepared.duration_ms,
            shortcut: old_assignment.sound.shortcut.clone(),
        };
        if let Err(error) = self.repository.save(&next) {
            rollback_import(&prepared, self.audio.as_ref());
            return Err(error);
        }

        let old_sound_id = old_assignment.sound.id.to_string();
        let inherited = lock(&self.runtime)
            .sounds
            .get(&old_sound_id)
            .cloned()
            .unwrap_or_else(|| runtime_for_loaded_sound(self.audio.is_available(), None, None));
        *lock(&self.state) = next;
        {
            let mut runtime = lock(&self.runtime);
            runtime.sounds.remove(&old_sound_id);
            runtime.sounds.insert(
                prepared.id.to_string(),
                SoundRuntime {
                    playable: self.audio.is_available(),
                    problem: if self.audio.is_available() {
                        None
                    } else {
                        Some(audio_device_problem())
                    },
                    shortcut_status: inherited.shortcut_status,
                    hotkey_id: inherited.hotkey_id,
                },
            );
        }
        if let Some(id) = inherited.hotkey_id {
            let replacement = lock(&self.state).assignment(&cell_id).cloned();
            if let Some(replacement) = replacement {
                self.hotkeys.activate(
                    id,
                    HotkeyTarget {
                        request: playback_request(&replacement, Trigger::GlobalShortcut),
                    },
                );
            }
        }
        self.audio.unload(&old_sound_id);
        if let Ok(path) = self
            .repository
            .audio_path(&old_assignment.sound.stored_file_name)
        {
            let _ = fs::remove_file(path);
        }
        Ok(Some(self.snapshot()))
    }

    pub fn play_sound(&self, cell_id: String, trigger: Trigger) -> Result<PlayResult, ApiError> {
        self.ensure_available()?;
        if trigger == Trigger::GlobalShortcut {
            return Err(ApiError::internal());
        }
        let assignment = {
            let state = lock(&self.state);
            validate_cell_in_grid(&cell_id, &state.grid)?;
            state
                .assignment(&cell_id)
                .cloned()
                .ok_or_else(|| ApiError::new("CELL_EMPTY", "That cell does not contain a sound."))?
        };
        let runtime = lock(&self.runtime)
            .sounds
            .get(&assignment.sound.id.to_string())
            .cloned();
        if let Some(runtime) = runtime
            && !runtime.playable
        {
            let problem = runtime.problem.unwrap_or_else(audio_device_problem);
            return Err(ApiError::new(problem.code, problem.message));
        }
        let instance_id = self.audio.play(playback_request(&assignment, trigger))?;
        Ok(PlayResult { instance_id })
    }

    pub fn delete_sound(&self, cell_id: String) -> Result<AppSnapshot, ApiError> {
        self.ensure_available()?;
        let _gate = lock(&self.mutation_gate);
        let old = {
            let state = lock(&self.state);
            validate_cell_in_grid(&cell_id, &state.grid)?;
            state
                .assignment(&cell_id)
                .cloned()
                .ok_or_else(|| ApiError::new("CELL_EMPTY", "That cell does not contain a sound."))?
        };
        let sound_id = old.sound.id.to_string();
        let old_runtime = lock(&self.runtime).sounds.get(&sound_id).cloned();

        if let (Some(shortcut), Some(id)) = (
            old.sound.shortcut.as_ref(),
            old_runtime.as_ref().and_then(|runtime| runtime.hotkey_id),
        ) {
            self.hotkeys.unregister(shortcut)?;
            self.hotkeys.deactivate(id);
        }

        let mut next = lock(&self.state).clone();
        next.assignments
            .retain(|assignment| assignment.cell_id != cell_id);
        if let Err(error) = self.repository.save(&next) {
            self.restore_hotkey(&old, old_runtime.as_ref());
            return Err(error);
        }
        *lock(&self.state) = next;
        lock(&self.runtime).sounds.remove(&sound_id);
        self.audio.unload(&sound_id);
        if let Ok(path) = self.repository.audio_path(&old.sound.stored_file_name) {
            let _ = fs::remove_file(path);
        }
        Ok(self.snapshot())
    }

    pub fn set_shortcut(
        &self,
        cell_id: String,
        input: ShortcutInput,
    ) -> Result<AppSnapshot, ApiError> {
        self.ensure_available()?;
        let shortcut = normalize_shortcut(input.into())?;
        let _gate = lock(&self.mutation_gate);
        let state = lock(&self.state).clone();
        validate_cell_in_grid(&cell_id, &state.grid)?;
        let assignment = state
            .assignment(&cell_id)
            .cloned()
            .ok_or_else(|| ApiError::new("CELL_EMPTY", "That cell does not contain a sound."))?;

        if let Some(conflict) = state.assignments.iter().find(|candidate| {
            candidate.cell_id != cell_id && candidate.sound.shortcut.as_ref() == Some(&shortcut)
        }) {
            let (row, column) = parse_cell_id(&conflict.cell_id)?;
            return Err(ApiError::with_details(
                "SHORTCUT_CONFLICT",
                "That shortcut is already assigned to another sound.",
                json!({
                    "shortcut": ShortcutDto::from(&shortcut),
                    "conflict": {
                        "cellId": conflict.cell_id,
                        "row": row,
                        "column": column,
                        "soundId": conflict.sound.id.to_string(),
                        "soundName": conflict.sound.display_name,
                    }
                }),
            ));
        }

        let current_runtime = lock(&self.runtime)
            .sounds
            .get(&assignment.sound.id.to_string())
            .cloned();
        if assignment.sound.shortcut.as_ref() == Some(&shortcut)
            && current_runtime
                .as_ref()
                .and_then(|runtime| runtime.shortcut_status)
                == Some(ShortcutStatus::Registered)
        {
            return Ok(self.snapshot());
        }

        let new_id = self.hotkeys.register(&shortcut)?;
        let mut next = state;
        next.assignment_mut(&cell_id)
            .expect("assignment was cloned from this state")
            .sound
            .shortcut = Some(shortcut.clone());
        if let Err(error) = self.repository.save(&next) {
            let _ = self.hotkeys.unregister(&shortcut);
            return Err(error);
        }

        *lock(&self.state) = next;
        {
            let mut runtime = lock(&self.runtime);
            let entry = runtime
                .sounds
                .entry(assignment.sound.id.to_string())
                .or_insert_with(|| runtime_for_loaded_sound(self.audio.is_available(), None, None));
            entry.shortcut_status = Some(ShortcutStatus::Registered);
            let old_id = entry.hotkey_id.replace(new_id);
            drop(runtime);

            self.hotkeys.activate(
                new_id,
                HotkeyTarget {
                    request: playback_request(
                        lock(&self.state)
                            .assignment(&cell_id)
                            .expect("committed assignment exists"),
                        Trigger::GlobalShortcut,
                    ),
                },
            );
            if let Some(old_id) = old_id {
                self.hotkeys.deactivate(old_id);
            }
        }
        if let Some(old) = assignment.sound.shortcut
            && old != shortcut
        {
            let _ = self.hotkeys.unregister(&old);
        }
        Ok(self.snapshot())
    }

    pub fn clear_shortcut(&self, cell_id: String) -> Result<AppSnapshot, ApiError> {
        self.ensure_available()?;
        let _gate = lock(&self.mutation_gate);
        let state = lock(&self.state).clone();
        validate_cell_in_grid(&cell_id, &state.grid)?;
        let assignment = state
            .assignment(&cell_id)
            .cloned()
            .ok_or_else(|| ApiError::new("CELL_EMPTY", "That cell does not contain a sound."))?;
        let Some(shortcut) = assignment.sound.shortcut.clone() else {
            return Ok(self.snapshot());
        };
        let runtime = lock(&self.runtime)
            .sounds
            .get(&assignment.sound.id.to_string())
            .cloned();
        if let Some(id) = runtime.as_ref().and_then(|runtime| runtime.hotkey_id) {
            self.hotkeys.unregister(&shortcut)?;
            self.hotkeys.deactivate(id);
        }

        let mut next = state;
        next.assignment_mut(&cell_id)
            .expect("assignment was cloned from this state")
            .sound
            .shortcut = None;
        if let Err(error) = self.repository.save(&next) {
            self.restore_hotkey(&assignment, runtime.as_ref());
            return Err(error);
        }
        *lock(&self.state) = next;
        if let Some(runtime) = lock(&self.runtime)
            .sounds
            .get_mut(&assignment.sound.id.to_string())
        {
            runtime.shortcut_status = None;
            runtime.hotkey_id = None;
        }
        Ok(self.snapshot())
    }

    pub fn resize_grid(&self, rows: u8, columns: u8) -> Result<AppSnapshot, ApiError> {
        self.ensure_available()?;
        validate_grid(rows, columns)?;
        let _gate = lock(&self.mutation_gate);
        let mut next = lock(&self.state).clone();
        let mut moving = next
            .assignments
            .iter()
            .enumerate()
            .filter_map(|assignment| {
                let (index, assignment) = assignment;
                let (row, column) = parse_cell_id(&assignment.cell_id).ok()?;
                (row >= rows || column >= columns).then(|| {
                    (
                        (row, column),
                        index,
                        json!({
                            "cellId": assignment.cell_id,
                            "row": row,
                            "column": column,
                            "soundId": assignment.sound.id.to_string(),
                            "soundName": assignment.sound.display_name,
                        }),
                    )
                })
            })
            .collect::<Vec<_>>();
        moving.sort_by_key(|(position, _, _)| *position);

        let occupied = next
            .assignments
            .iter()
            .filter_map(|assignment| {
                let (row, column) = parse_cell_id(&assignment.cell_id).ok()?;
                (row < rows && column < columns).then_some((row, column))
            })
            .collect::<std::collections::HashSet<_>>();
        let empty_cells = (0..rows)
            .flat_map(|row| (0..columns).map(move |column| (row, column)))
            .filter(|position| !occupied.contains(position))
            .collect::<Vec<_>>();

        if moving.len() > empty_cells.len() {
            let blockers = moving
                .into_iter()
                .map(|(_, _, blocker)| blocker)
                .collect::<Vec<_>>();
            return Err(ApiError::with_details(
                "GRID_SHRINK_BLOCKED",
                "The requested grid does not have enough cells for every sound.",
                json!({
                    "requested": { "rows": rows, "columns": columns },
                    "soundCount": next.assignments.len(),
                    "availableCells": usize::from(rows) * usize::from(columns),
                    "blockingCells": blockers,
                }),
            ));
        }

        let mut moved_sound_ids = Vec::with_capacity(moving.len());
        for ((_, index, _), (row, column)) in moving.into_iter().zip(empty_cells) {
            let assignment = &mut next.assignments[index];
            assignment.cell_id = format!("r{row}c{column}");
            moved_sound_ids.push(assignment.sound.id.to_string());
        }
        next.grid.rows = rows;
        next.grid.columns = columns;
        next.sort_assignments();
        self.repository.save(&next)?;

        let moved_assignments = next
            .assignments
            .iter()
            .filter(|assignment| moved_sound_ids.contains(&assignment.sound.id.to_string()))
            .cloned()
            .collect::<Vec<_>>();
        *lock(&self.state) = next;
        for assignment in moved_assignments {
            let hotkey_id = lock(&self.runtime)
                .sounds
                .get(&assignment.sound.id.to_string())
                .and_then(|runtime| runtime.hotkey_id);
            if let Some(id) = hotkey_id {
                self.hotkeys.activate(
                    id,
                    HotkeyTarget {
                        request: playback_request(&assignment, Trigger::GlobalShortcut),
                    },
                );
            }
        }
        Ok(self.snapshot())
    }

    fn restore_hotkey(&self, assignment: &Assignment, previous_runtime: Option<&SoundRuntime>) {
        let Some(shortcut) = assignment.sound.shortcut.as_ref() else {
            return;
        };
        let Some(previous_runtime) = previous_runtime else {
            return;
        };
        if previous_runtime.hotkey_id.is_none() {
            return;
        }
        let Ok(id) = self.hotkeys.register(shortcut) else {
            if let Some(runtime) = lock(&self.runtime)
                .sounds
                .get_mut(&assignment.sound.id.to_string())
            {
                runtime.shortcut_status = Some(ShortcutStatus::Unavailable);
                runtime.hotkey_id = None;
            }
            return;
        };
        self.hotkeys.activate(
            id,
            HotkeyTarget {
                request: playback_request(assignment, Trigger::GlobalShortcut),
            },
        );
        if let Some(runtime) = lock(&self.runtime)
            .sounds
            .get_mut(&assignment.sound.id.to_string())
        {
            runtime.shortcut_status = previous_runtime
                .shortcut_status
                .or(Some(ShortcutStatus::Registered));
            runtime.hotkey_id = Some(id);
        }
    }

    fn snapshot(&self) -> AppSnapshot {
        let state = lock(&self.state).clone();
        let runtime = lock(&self.runtime);
        let mut snapshot = empty_snapshot(&state);
        snapshot.warnings = runtime.startup_warnings.clone();

        for row in 0..state.grid.rows {
            for column in 0..state.grid.columns {
                let cell_id = format!("r{row}c{column}");
                let sound = state.assignment(&cell_id).map(|assignment| {
                    let sound_id = assignment.sound.id.to_string();
                    let status = runtime.sounds.get(&sound_id);
                    if let Some(problem) = status.and_then(|status| status.problem.clone()) {
                        snapshot.warnings.push(AppWarningDto {
                            code: problem.code.clone(),
                            message: problem.message.clone(),
                            cell_id: Some(cell_id.clone()),
                        });
                    }
                    if status.and_then(|status| status.shortcut_status)
                        == Some(ShortcutStatus::Unavailable)
                    {
                        snapshot.warnings.push(AppWarningDto {
                            code: "SHORTCUT_UNAVAILABLE".to_owned(),
                            message: format!(
                                "The shortcut for “{}” could not be registered. It may be reserved by the operating system or another app.",
                                assignment.sound.display_name
                            ),
                            cell_id: Some(cell_id.clone()),
                        });
                    }
                    SoundDto {
                        id: sound_id,
                        display_name: assignment.sound.display_name.clone(),
                        format: assignment.sound.format,
                        duration_ms: assignment.sound.duration_ms,
                        shortcut: assignment.sound.shortcut.as_ref().map(ShortcutDto::from),
                        shortcut_status: assignment
                            .sound
                            .shortcut
                            .as_ref()
                            .and_then(|_| status.and_then(|runtime| runtime.shortcut_status)),
                        playable: status.is_some_and(|status| status.playable),
                        problem: status.and_then(|status| status.problem.clone()),
                    }
                });
                snapshot.cells.push(CellDto {
                    cell_id,
                    row,
                    column,
                    sound,
                });
            }
        }
        snapshot
    }
}

fn playback_request(assignment: &Assignment, trigger: Trigger) -> PlaybackRequest {
    PlaybackRequest {
        sound_id: assignment.sound.id.to_string(),
        cell_id: assignment.cell_id.clone(),
        trigger,
    }
}

fn audio_device_problem() -> ProblemDto {
    ProblemDto {
        code: "AUDIO_DEVICE_UNAVAILABLE".to_owned(),
        message: "The default audio output device is unavailable.".to_owned(),
    }
}

fn runtime_for_loaded_sound(
    audio_available: bool,
    shortcut_status: Option<ShortcutStatus>,
    hotkey_id: Option<HotkeyId>,
) -> SoundRuntime {
    SoundRuntime {
        playable: audio_available,
        problem: (!audio_available).then(audio_device_problem),
        shortcut_status,
        hotkey_id,
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet, VecDeque};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

    use tempfile::TempDir;
    use uuid::Uuid;

    use super::*;
    use crate::domain::{AudioFormat, Grid, Modifier, Shortcut};
    use crate::hotkeys::HotkeyTarget;
    use crate::ports::{AudioMetadata, PlaybackRequest, RepositoryLoad};

    struct FakeRepository {
        _directory: TempDir,
        audio_dir: PathBuf,
        state: Mutex<PersistedState>,
        fail_save: AtomicBool,
    }

    impl FakeRepository {
        fn new(state: PersistedState) -> Arc<Self> {
            let directory = tempfile::tempdir().unwrap();
            let audio_dir = directory.path().join("audio");
            fs::create_dir(&audio_dir).unwrap();
            for assignment in &state.assignments {
                fs::write(
                    audio_dir.join(&assignment.sound.stored_file_name),
                    b"managed",
                )
                .unwrap();
            }
            Arc::new(Self {
                _directory: directory,
                audio_dir,
                state: Mutex::new(state),
                fail_save: AtomicBool::new(false),
            })
        }

        fn fail_next_save(&self) {
            self.fail_save.store(true, Ordering::Release);
        }

        fn source(&self, name: &str) -> PathBuf {
            let path = self.audio_dir.parent().unwrap().join(name);
            fs::write(&path, b"fake audio").unwrap();
            path
        }
    }

    impl StateRepository for FakeRepository {
        fn load(&self) -> Result<RepositoryLoad, ApiError> {
            Ok(RepositoryLoad {
                state: lock(&self.state).clone(),
                warnings: Vec::new(),
            })
        }

        fn save(&self, state: &PersistedState) -> Result<(), ApiError> {
            if self.fail_save.swap(false, Ordering::AcqRel) {
                return Err(ApiError::persistence());
            }
            *lock(&self.state) = state.clone();
            Ok(())
        }

        fn audio_dir(&self) -> &Path {
            &self.audio_dir
        }

        fn audio_path(&self, stored_file_name: &str) -> Result<PathBuf, ApiError> {
            Ok(self.audio_dir.join(stored_file_name))
        }
    }

    #[derive(Default)]
    struct FakeAudio {
        available: AtomicBool,
        fail_all_loads: AtomicBool,
        loaded: Mutex<HashSet<String>>,
        fail_load: Mutex<HashSet<String>>,
        plays: Mutex<Vec<PlaybackRequest>>,
        next_instance: AtomicUsize,
    }

    impl FakeAudio {
        fn available() -> Arc<Self> {
            Arc::new(Self {
                available: AtomicBool::new(true),
                ..Self::default()
            })
        }
    }

    impl AudioService for FakeAudio {
        fn is_available(&self) -> bool {
            self.available.load(Ordering::Acquire)
        }

        fn probe(&self, _path: &Path) -> Result<AudioMetadata, ApiError> {
            Ok(AudioMetadata { duration_ms: 900 })
        }

        fn load(&self, sound_id: &str, _path: &Path) -> Result<AudioMetadata, ApiError> {
            if self.fail_all_loads.load(Ordering::Acquire)
                || lock(&self.fail_load).contains(sound_id)
            {
                return Err(ApiError::decode());
            }
            lock(&self.loaded).insert(sound_id.to_owned());
            Ok(AudioMetadata { duration_ms: 900 })
        }

        fn unload(&self, sound_id: &str) {
            lock(&self.loaded).remove(sound_id);
        }

        fn play(&self, request: PlaybackRequest) -> Result<String, ApiError> {
            lock(&self.plays).push(request);
            Ok(format!(
                "instance-{}",
                self.next_instance.fetch_add(1, Ordering::AcqRel)
            ))
        }

        fn try_play(&self, request: PlaybackRequest) {
            lock(&self.plays).push(request);
        }
    }

    #[derive(Default)]
    struct FakeHotkeys {
        fail_register: AtomicBool,
        capture_active: AtomicBool,
        register_calls: AtomicUsize,
        next_id: AtomicU32,
        registered: Mutex<HashMap<Shortcut, HotkeyId>>,
        active: Mutex<HashMap<HotkeyId, HotkeyTarget>>,
    }

    impl HotkeyService for FakeHotkeys {
        fn register(&self, shortcut: &Shortcut) -> Result<HotkeyId, ApiError> {
            self.register_calls.fetch_add(1, Ordering::AcqRel);
            if self.fail_register.load(Ordering::Acquire) {
                return Err(ApiError::with_details(
                    "SHORTCUT_UNAVAILABLE",
                    "Shortcut unavailable.",
                    json!({ "shortcut": ShortcutDto::from(shortcut) }),
                ));
            }
            if let Some(id) = lock(&self.registered).get(shortcut).copied() {
                return Ok(id);
            }
            let id = self.next_id.fetch_add(1, Ordering::AcqRel) + 1;
            lock(&self.registered).insert(shortcut.clone(), id);
            Ok(id)
        }

        fn unregister(&self, shortcut: &Shortcut) -> Result<(), ApiError> {
            lock(&self.registered).remove(shortcut);
            Ok(())
        }

        fn activate(&self, id: HotkeyId, target: HotkeyTarget) {
            lock(&self.active).insert(id, target);
        }

        fn deactivate(&self, id: HotkeyId) {
            lock(&self.active).remove(&id);
        }

        fn set_capture_active(&self, active: bool) {
            self.capture_active.store(active, Ordering::Release);
        }
    }

    #[derive(Default)]
    struct FakePicker {
        selections: Mutex<VecDeque<Option<PathBuf>>>,
    }

    impl FakePicker {
        fn select(&self, path: Option<PathBuf>) {
            lock(&self.selections).push_back(path);
        }
    }

    impl FilePicker for FakePicker {
        fn pick_audio_file(&self) -> Result<Option<PathBuf>, ApiError> {
            Ok(lock(&self.selections).pop_front().unwrap_or(None))
        }
    }

    struct Harness {
        coordinator: Arc<Coordinator>,
        repository: Arc<FakeRepository>,
        audio: Arc<FakeAudio>,
        hotkeys: Arc<FakeHotkeys>,
        picker: Arc<FakePicker>,
    }

    impl Harness {
        fn new(state: PersistedState) -> Self {
            let repository = FakeRepository::new(state);
            let audio = FakeAudio::available();
            let hotkeys = Arc::new(FakeHotkeys::default());
            let picker = Arc::new(FakePicker::default());
            let load = repository.load().unwrap();
            let repository_port: Arc<dyn StateRepository> = repository.clone();
            let audio_port: Arc<dyn AudioService> = audio.clone();
            let hotkey_port: Arc<dyn HotkeyService> = hotkeys.clone();
            let picker_port: Arc<dyn FilePicker> = picker.clone();
            let coordinator = Coordinator::initialize(
                load,
                repository_port,
                audio_port,
                hotkey_port,
                picker_port,
            );
            Self {
                coordinator,
                repository,
                audio,
                hotkeys,
                picker,
            }
        }
    }

    fn shortcut(modifier: Modifier, code: &str) -> Shortcut {
        Shortcut {
            modifiers: vec![modifier],
            code: code.into(),
        }
    }

    fn assignment(cell_id: &str, name: &str, shortcut: Option<Shortcut>) -> Assignment {
        let id = Uuid::new_v4();
        Assignment {
            cell_id: cell_id.into(),
            sound: Sound {
                id,
                display_name: name.into(),
                original_file_name: format!("{name}.mp3"),
                stored_file_name: format!("{id}.mp3"),
                format: AudioFormat::Mp3,
                duration_ms: 500,
                shortcut,
            },
        }
    }

    fn state_with(assignments: Vec<Assignment>, rows: u8, columns: u8) -> PersistedState {
        PersistedState {
            schema_version: 1,
            grid: Grid { rows, columns },
            assignments,
        }
    }

    #[test]
    fn conflict_details_are_exact_and_backend_authoritative() {
        let existing = assignment("r1c2", "Air horn", Some(shortcut(Modifier::Alt, "KeyF")));
        let target = assignment("r0c0", "Target", None);
        let harness = Harness::new(state_with(vec![target, existing.clone()], 3, 3));

        let error = harness
            .coordinator
            .set_shortcut(
                "r0c0".into(),
                ShortcutInput {
                    modifiers: vec![Modifier::Alt],
                    code: "KeyF".into(),
                },
            )
            .unwrap_err();
        assert_eq!(error.code, "SHORTCUT_CONFLICT");
        let details = error.details.unwrap();
        assert_eq!(
            details["shortcut"]["display"],
            ShortcutDto::from(existing.sound.shortcut.as_ref().unwrap()).display
        );
        assert_eq!(details["conflict"]["cellId"], "r1c2");
        assert_eq!(details["conflict"]["row"], 1);
        assert_eq!(details["conflict"]["column"], 2);
        assert_eq!(details["conflict"]["soundName"], "Air horn");
    }

    #[test]
    fn registered_shortcut_reassignment_is_idempotent() {
        let existing = shortcut(Modifier::Control, "KeyA");
        let harness = Harness::new(state_with(
            vec![assignment("r0c0", "Air horn", Some(existing.clone()))],
            1,
            1,
        ));
        let registrations = harness.hotkeys.register_calls.load(Ordering::Acquire);
        harness
            .coordinator
            .set_shortcut(
                "r0c0".into(),
                ShortcutInput {
                    modifiers: existing.modifiers,
                    code: existing.code,
                },
            )
            .unwrap();
        assert_eq!(
            harness.hotkeys.register_calls.load(Ordering::Acquire),
            registrations
        );
    }

    #[test]
    fn os_registration_failure_preserves_the_old_shortcut_and_state() {
        let old = shortcut(Modifier::Control, "KeyA");
        let harness = Harness::new(state_with(
            vec![assignment("r0c0", "Air horn", Some(old.clone()))],
            1,
            1,
        ));
        harness.hotkeys.fail_register.store(true, Ordering::Release);
        let error = harness
            .coordinator
            .set_shortcut(
                "r0c0".into(),
                ShortcutInput {
                    modifiers: vec![Modifier::Alt],
                    code: "KeyB".into(),
                },
            )
            .unwrap_err();
        assert_eq!(error.code, "SHORTCUT_UNAVAILABLE");
        assert_eq!(
            lock(&harness.repository.state).assignments[0]
                .sound
                .shortcut,
            Some(old.clone())
        );
        assert!(lock(&harness.hotkeys.registered).contains_key(&old));
    }

    #[test]
    fn picker_cancellation_is_a_silent_no_op() {
        let harness = Harness::new(PersistedState::default());
        harness.picker.select(None);
        assert_eq!(
            harness
                .coordinator
                .pick_and_import_sound("r0c0".into())
                .unwrap(),
            None
        );
        assert!(lock(&harness.repository.state).assignments.is_empty());
    }

    #[test]
    fn persistence_failure_rolls_back_import_and_replace() {
        let harness = Harness::new(PersistedState::default());
        let source = harness.repository.source("new.mp3");
        harness.picker.select(Some(source));
        harness.repository.fail_next_save();
        assert_eq!(
            harness
                .coordinator
                .pick_and_import_sound("r0c0".into())
                .unwrap_err()
                .code,
            "PERSISTENCE_FAILED"
        );
        assert!(lock(&harness.audio.loaded).is_empty());
        assert_eq!(
            fs::read_dir(&harness.repository.audio_dir).unwrap().count(),
            0
        );

        let old = assignment("r0c0", "Old sound", Some(shortcut(Modifier::Alt, "KeyF")));
        let harness = Harness::new(state_with(vec![old.clone()], 1, 1));
        let replacement_source = harness.repository.source("replacement.wav");
        harness.picker.select(Some(replacement_source));
        harness.repository.fail_next_save();
        assert!(
            harness
                .coordinator
                .pick_and_replace_sound("r0c0".into())
                .is_err()
        );
        let saved = lock(&harness.repository.state).clone();
        assert_eq!(saved.assignments[0].sound.id, old.sound.id);
        assert_eq!(saved.assignments[0].sound.shortcut, old.sound.shortcut);
        assert_eq!(lock(&harness.audio.loaded).len(), 1);
    }

    #[test]
    fn persistence_failure_restores_delete_and_shortcut_transactions() {
        let old_shortcut = shortcut(Modifier::Control, "KeyA");
        let old = assignment("r0c0", "Air horn", Some(old_shortcut.clone()));
        let harness = Harness::new(state_with(vec![old.clone()], 1, 1));
        let source_path = harness.repository.source("user-source.mp3");
        harness.repository.fail_next_save();
        assert!(harness.coordinator.delete_sound("r0c0".into()).is_err());
        assert!(
            source_path.exists(),
            "delete must never touch a source file"
        );
        assert!(
            harness
                .repository
                .audio_dir
                .join(&old.sound.stored_file_name)
                .exists()
        );
        assert!(lock(&harness.hotkeys.registered).contains_key(&old_shortcut));
        assert_eq!(
            harness.coordinator.get_state().unwrap().cells[0]
                .sound
                .as_ref()
                .unwrap()
                .display_name,
            "Air horn"
        );

        harness.repository.fail_next_save();
        let new_shortcut = shortcut(Modifier::Alt, "KeyB");
        assert!(
            harness
                .coordinator
                .set_shortcut(
                    "r0c0".into(),
                    ShortcutInput {
                        modifiers: new_shortcut.modifiers.clone(),
                        code: new_shortcut.code.clone(),
                    },
                )
                .is_err()
        );
        assert!(lock(&harness.hotkeys.registered).contains_key(&old_shortcut));
        assert!(!lock(&harness.hotkeys.registered).contains_key(&new_shortcut));

        harness.repository.fail_next_save();
        assert!(harness.coordinator.clear_shortcut("r0c0".into()).is_err());
        assert!(lock(&harness.hotkeys.registered).contains_key(&old_shortcut));
        assert_eq!(
            harness.coordinator.get_state().unwrap().cells[0]
                .sound
                .as_ref()
                .unwrap()
                .shortcut,
            Some(ShortcutDto::from(&old_shortcut))
        );
    }

    #[test]
    fn successful_delete_removes_only_the_managed_copy() {
        let old = assignment("r0c0", "Air horn", None);
        let harness = Harness::new(state_with(vec![old.clone()], 1, 1));
        let source_path = harness.repository.source("user-source.mp3");
        let managed_path = harness
            .repository
            .audio_dir
            .join(&old.sound.stored_file_name);

        let snapshot = harness.coordinator.delete_sound("r0c0".into()).unwrap();

        assert!(source_path.exists());
        assert!(!managed_path.exists());
        assert!(snapshot.cells[0].sound.is_none());
    }

    #[test]
    fn replacement_success_keeps_the_shortcut() {
        let existing_shortcut = shortcut(Modifier::Alt, "KeyF");
        let old = assignment("r0c0", "Old", Some(existing_shortcut.clone()));
        let harness = Harness::new(state_with(vec![old], 1, 1));
        let source = harness.repository.source("Replacement.flac");
        harness.picker.select(Some(source));
        let snapshot = harness
            .coordinator
            .pick_and_replace_sound("r0c0".into())
            .unwrap()
            .unwrap();
        assert_eq!(
            snapshot.cells[0].sound.as_ref().unwrap().shortcut,
            Some(ShortcutDto::from(&existing_shortcut))
        );
    }

    #[test]
    fn replacement_load_failure_leaves_old_sound_usable() {
        let existing_shortcut = shortcut(Modifier::Alt, "KeyF");
        let old = assignment("r0c0", "Old", Some(existing_shortcut.clone()));
        let harness = Harness::new(state_with(vec![old.clone()], 1, 1));
        let old_path = harness
            .repository
            .audio_dir
            .join(&old.sound.stored_file_name);
        harness.audio.fail_all_loads.store(true, Ordering::Release);
        let source = harness.repository.source("Replacement.flac");
        harness.picker.select(Some(source));

        let error = harness
            .coordinator
            .pick_and_replace_sound("r0c0".into())
            .unwrap_err();

        assert_eq!(error.code, "AUDIO_DECODE_FAILED");
        assert!(old_path.exists());
        assert!(lock(&harness.audio.loaded).contains(&old.sound.id.to_string()));
        let snapshot = harness.coordinator.get_state().unwrap();
        let sound = snapshot.cells[0].sound.as_ref().unwrap();
        assert_eq!(sound.id, old.sound.id.to_string());
        assert_eq!(sound.shortcut, Some(ShortcutDto::from(&existing_shortcut)));
    }

    #[test]
    fn grid_shrink_relocates_outside_sounds_and_retargets_shortcuts() {
        let lower = assignment("r3c0", "Lower", Some(shortcut(Modifier::Control, "KeyL")));
        let upper = assignment("r2c3", "Upper", None);
        let lower_id = lower.sound.id.to_string();
        let harness = Harness::new(state_with(vec![lower, upper], 4, 4));

        let resized = harness.coordinator.resize_grid(2, 2).unwrap();

        assert_eq!(resized.cells.len(), 4);
        assert_eq!(
            resized.cells[0].sound.as_ref().unwrap().display_name,
            "Upper"
        );
        assert_eq!(
            resized.cells[1].sound.as_ref().unwrap().display_name,
            "Lower"
        );
        let lower_target = lock(&harness.hotkeys.active)
            .values()
            .find(|target| target.request.sound_id == lower_id)
            .cloned()
            .unwrap();
        assert_eq!(lower_target.request.cell_id, "r0c1");

        let expanded = harness.coordinator.resize_grid(5, 6).unwrap();
        assert_eq!(expanded.cells.len(), 30);
        assert_eq!(
            expanded.cells[0].sound.as_ref().unwrap().display_name,
            "Upper"
        );
        assert_eq!(
            expanded.cells[1].sound.as_ref().unwrap().display_name,
            "Lower"
        );
    }

    #[test]
    fn grid_shrink_lists_all_unplaceable_sounds_row_major() {
        let assignments = vec![
            assignment("r0c0", "One", None),
            assignment("r0c1", "Two", None),
            assignment("r1c0", "Three", None),
            assignment("r1c1", "Four", None),
            assignment("r2c3", "Upper", None),
            assignment("r3c0", "Lower", None),
        ];
        let harness = Harness::new(state_with(assignments, 4, 4));
        let error = harness.coordinator.resize_grid(2, 2).unwrap_err();
        let blockers = error.details.unwrap()["blockingCells"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(blockers.len(), 2);
        assert_eq!(blockers[0]["cellId"], "r2c3");
        assert_eq!(blockers[1]["cellId"], "r3c0");
        assert_eq!(error.code, "GRID_SHRINK_BLOCKED");
        assert_eq!(lock(&harness.repository.state).grid.rows, 4);
    }

    #[test]
    fn shortcut_capture_mode_suspends_and_restores_native_registrations() {
        let assigned_shortcut = shortcut(Modifier::Alt, "KeyF");
        let harness = Harness::new(state_with(
            vec![assignment(
                "r0c0",
                "Air horn",
                Some(assigned_shortcut.clone()),
            )],
            1,
            1,
        ));
        assert!(
            lock(&harness.hotkeys.registered).contains_key(&assigned_shortcut),
            "startup registers the shortcut"
        );

        harness
            .coordinator
            .set_shortcut_capture_active(true)
            .unwrap();
        assert!(harness.hotkeys.capture_active.load(Ordering::Acquire));
        assert!(lock(&harness.hotkeys.registered).is_empty());
        assert!(lock(&harness.hotkeys.active).is_empty());

        harness
            .coordinator
            .set_shortcut_capture_active(false)
            .unwrap();
        assert!(!harness.hotkeys.capture_active.load(Ordering::Acquire));
        assert!(lock(&harness.hotkeys.registered).contains_key(&assigned_shortcut));
        assert_eq!(lock(&harness.hotkeys.active).len(), 1);
    }

    #[test]
    fn shortcut_conflict_remains_authoritative_while_native_keys_are_suspended() {
        let assigned_shortcut = shortcut(Modifier::Alt, "KeyF");
        let harness = Harness::new(state_with(
            vec![
                assignment("r0c0", "Air horn", Some(assigned_shortcut.clone())),
                assignment("r0c1", "Target", None),
            ],
            1,
            2,
        ));
        harness
            .coordinator
            .set_shortcut_capture_active(true)
            .unwrap();

        let error = harness
            .coordinator
            .set_shortcut(
                "r0c1".into(),
                ShortcutInput {
                    modifiers: assigned_shortcut.modifiers.clone(),
                    code: assigned_shortcut.code.clone(),
                },
            )
            .unwrap_err();

        assert_eq!(error.code, "SHORTCUT_CONFLICT");
        assert_eq!(error.details.unwrap()["conflict"]["soundName"], "Air horn");
        assert!(lock(&harness.hotkeys.registered).is_empty());
        assert!(lock(&harness.audio.plays).is_empty());

        harness
            .coordinator
            .set_shortcut_capture_active(false)
            .unwrap();
        assert!(lock(&harness.hotkeys.registered).contains_key(&assigned_shortcut));
    }

    #[test]
    fn one_missing_audio_file_does_not_affect_other_cells() {
        let missing = assignment("r0c0", "Missing", None);
        let healthy = assignment("r0c1", "Healthy", None);
        let repository =
            FakeRepository::new(state_with(vec![missing.clone(), healthy.clone()], 1, 2));
        let audio = FakeAudio::available();
        lock(&audio.fail_load).insert(missing.sound.id.to_string());
        let hotkeys = Arc::new(FakeHotkeys::default());
        let picker = Arc::new(FakePicker::default());
        let coordinator = Coordinator::initialize(
            repository.load().unwrap(),
            repository as Arc<dyn StateRepository>,
            audio as Arc<dyn AudioService>,
            hotkeys as Arc<dyn HotkeyService>,
            picker as Arc<dyn FilePicker>,
        );
        let snapshot = coordinator.get_state().unwrap();
        assert!(!snapshot.cells[0].sound.as_ref().unwrap().playable);
        assert!(snapshot.cells[1].sound.as_ref().unwrap().playable);
        assert_eq!(snapshot.warnings.len(), 1);
    }

    #[test]
    fn twenty_rapid_plays_are_twenty_independent_requests() {
        let sound = assignment("r0c0", "Air horn", None);
        let sound_id = sound.sound.id.to_string();
        let harness = Harness::new(state_with(vec![sound], 1, 1));
        let mut instances = HashSet::new();
        for _ in 0..20 {
            instances.insert(
                harness
                    .coordinator
                    .play_sound("r0c0".into(), Trigger::Pointer)
                    .unwrap()
                    .instance_id,
            );
        }
        assert_eq!(instances.len(), 20);
        let plays = lock(&harness.audio.plays);
        assert_eq!(plays.len(), 20);
        assert!(plays.iter().all(|play| {
            play.sound_id == sound_id && play.cell_id == "r0c0" && play.trigger == Trigger::Pointer
        }));
    }
}
