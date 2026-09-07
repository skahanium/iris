//! Durable, vault-local checkpoints for note and folder moves.
//!
//! The journal records paths, hashes and database row identities only. Note
//! bodies remain in their authoritative Markdown files or existing protected
//! versions and are never serialized into the operation record.
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::AppState;
use crate::cas::hash::content_hash;
use crate::error::{AppError, AppResult};
use crate::indexer::scan::rename_file_index;
use crate::storage::atomic_write::{
    atomic_create, atomic_write, link_file_no_replace_locked, remove_file_durable,
    sync_parent_directory,
};
use crate::storage::note_move::{rewrite_wikilinks, BacklinkEdit};
use crate::storage::paths::{relative_path, resolve_vault_path};

const JOURNAL_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MoveKind {
    File,
    Folder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Prepared,
    MovingFilesystem,
    FilesystemComplete,
    RollingBack,
    IdentityComplete,
    BacklinksComplete,
    PendingConflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileState {
    Planned,
    Linking,
    Linked,
    Moved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BacklinkState {
    Planned,
    LinkingMaterial,
    MaterialLinked,
    Displaced,
    Published,
    Pending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PhysicalFile {
    from: String,
    to: String,
    hash: String,
    state: FileState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentityMove {
    from: String,
    to: String,
    file_id: Option<i64>,
    stale_target_file_id: Option<i64>,
    version_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BacklinkIntent {
    path: String,
    before_hash: String,
    after_hash: String,
    recovery_version_id: i64,
    state: BacklinkState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MoveCheckpoint {
    schema_version: u32,
    operation_id: String,
    vault: String,
    kind: MoveKind,
    old_root: String,
    new_root: String,
    phase: Phase,
    files: Vec<PhysicalFile>,
    directories: Vec<(String, String)>,
    identities: Vec<IdentityMove>,
    mappings: Vec<(String, String)>,
    backlinks: Vec<BacklinkIntent>,
    #[serde(skip)]
    journal_path: PathBuf,
}

#[derive(Debug, Default)]
pub(crate) struct BacklinkOutcome {
    pub(crate) applied_paths: Vec<String>,
    pub(crate) pending_paths: Vec<String>,
    pub(crate) recovery_warnings: Vec<String>,
}

type PhysicalInventory = (Vec<PhysicalFile>, Vec<(String, String)>);

impl MoveCheckpoint {
    pub(crate) fn create(
        state: &AppState,
        vault: &Path,
        kind: MoveKind,
        roots: (&str, &str),
        mappings: &BTreeMap<String, String>,
        backlinks: &[BacklinkEdit],
        recovery_versions: &[(String, i64)],
    ) -> AppResult<Self> {
        let (old_root, new_root) = roots;
        let vault = vault.canonicalize()?;
        let (files, directories) = collect_physical_items(&vault, kind, old_root, new_root)?;
        let identities = state.db.with_read_conn(|connection| {
            mappings
                .iter()
                .map(|(from, to)| capture_identity(connection, &vault, from, to))
                .collect::<AppResult<Vec<_>>>()
        })?;
        let recovery_by_path: BTreeMap<_, _> = recovery_versions.iter().cloned().collect();
        let backlinks = backlinks
            .iter()
            .map(|edit| {
                let path = mappings
                    .get(&edit.path)
                    .cloned()
                    .unwrap_or_else(|| edit.path.clone());
                let recovery_version_id = recovery_by_path
                    .get(&edit.path)
                    .copied()
                    .ok_or_else(|| AppError::msg("move_backlink_recovery_version_missing"))?;
                Ok(BacklinkIntent {
                    path,
                    before_hash: content_hash(edit.before.as_bytes()),
                    after_hash: content_hash(edit.after.as_bytes()),
                    recovery_version_id,
                    state: BacklinkState::Planned,
                })
            })
            .collect::<AppResult<Vec<_>>>()?;
        let operation_id = Uuid::new_v4().to_string();
        let journal_path = journal_directory(&vault).join(format!("{operation_id}.json"));
        let checkpoint = Self {
            schema_version: JOURNAL_VERSION,
            operation_id,
            vault: vault.to_string_lossy().into_owned(),
            kind,
            old_root: old_root.into(),
            new_root: new_root.into(),
            phase: Phase::Prepared,
            files,
            directories,
            identities,
            mappings: mappings
                .iter()
                .map(|(a, b)| (a.clone(), b.clone()))
                .collect(),
            backlinks,
            journal_path,
        };
        checkpoint.persist()?;
        Ok(checkpoint)
    }

    pub(crate) fn move_filesystem(&mut self) -> AppResult<()> {
        self.phase = Phase::MovingFilesystem;
        self.persist()?;
        let vault = self.vault_path()?;
        for (_, target) in &self.directories {
            let target = resolve_vault_path(&vault, target)?;
            if target.exists() {
                if !target.is_dir() {
                    return Err(AppError::msg("move_recovery_target_conflict"));
                }
            } else {
                fs::create_dir(&target)?;
                sync_parent_directory(
                    target
                        .parent()
                        .ok_or_else(|| AppError::msg("move directory has no parent"))?,
                )?;
            }
        }
        for index in 0..self.files.len() {
            self.move_file_forward(index)?;
        }
        self.validate_target_tree()?;
        for (source, _) in self.directories.iter().rev() {
            let source = resolve_vault_path(&vault, source)?;
            if source.exists() {
                fs::remove_dir(&source)
                    .map_err(|_| AppError::msg("move_recovery_source_tree_conflict"))?;
                sync_parent_directory(
                    source
                        .parent()
                        .ok_or_else(|| AppError::msg("move directory has no parent"))?,
                )?;
            }
        }
        self.phase = Phase::FilesystemComplete;
        self.persist()
    }

    fn validate_target_tree(&self) -> AppResult<()> {
        if self.kind == MoveKind::File {
            return Ok(());
        }
        let vault = self.vault_path()?;
        let allowed_files: BTreeSet<_> = self.files.iter().map(|item| item.to.as_str()).collect();
        let allowed_directories: BTreeSet<_> = self
            .directories
            .iter()
            .map(|(_, target)| target.as_str())
            .collect();
        let root = resolve_vault_path(&vault, &self.new_root)?;
        let mut pending = vec![root];
        while let Some(directory) = pending.pop() {
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                let metadata = fs::symlink_metadata(entry.path())?;
                if metadata.file_type().is_symlink() {
                    return Err(AppError::msg("move_recovery_target_conflict"));
                }
                let relative = relative_path(&vault, &entry.path())?;
                if metadata.is_dir() {
                    if !allowed_directories.contains(relative.as_str()) {
                        return Err(AppError::msg("move_recovery_target_conflict"));
                    }
                    pending.push(entry.path());
                } else if !metadata.is_file() || !allowed_files.contains(relative.as_str()) {
                    return Err(AppError::msg("move_recovery_target_conflict"));
                }
            }
        }
        Ok(())
    }

    fn move_file_forward(&mut self, index: usize) -> AppResult<()> {
        self.ensure_physical_material(index)?;
        self.ensure_identity_witness(index)?;
        let vault = self.vault_path()?;
        let source = resolve_vault_path(&vault, &self.files[index].from)?;
        let target = resolve_vault_path(&vault, &self.files[index].to)?;
        let expected_hash = self.files[index].hash.clone();
        let source_hash = file_hash_if_regular(&source)?;
        let target_hash = file_hash_if_regular(&target)?;
        match (source_hash, target_hash) {
            (Some(source_hash), None) if source_hash == expected_hash => {
                self.files[index].state = FileState::Linking;
                self.persist()?;
                link_file_no_replace_locked(&source, &target)?;
                self.files[index].state = FileState::Linked;
                self.persist()?;
                if !same_file_identity(&source, &target)? {
                    return Err(AppError::msg("move_recovery_file_conflict"));
                }
                remove_file_durable(&source)?;
                self.files[index].state = FileState::Moved;
                self.persist()
            }
            (Some(source_hash), Some(target_hash))
                if source_hash == expected_hash
                    && target_hash == expected_hash
                    && matches!(
                        self.files[index].state,
                        FileState::Linking | FileState::Linked
                    )
                    && same_file_identity(&source, &target)? =>
            {
                self.files[index].state = FileState::Linked;
                self.persist()?;
                remove_file_durable(&source)?;
                self.files[index].state = FileState::Moved;
                self.persist()
            }
            (None, Some(target_hash))
                if target_hash == expected_hash
                    && matches!(
                        self.files[index].state,
                        FileState::Linked | FileState::Moved
                    ) =>
            {
                self.files[index].state = FileState::Moved;
                self.persist()
            }
            _ => Err(AppError::msg("move_recovery_file_conflict")),
        }
    }

    pub(crate) fn commit_identity(&mut self, state: &AppState) -> AppResult<()> {
        let vault = self.vault_path()?;
        self.validate_filesystem_for_identity()?;
        state.db.with_conn(|connection| {
            connection.execute_batch("BEGIN IMMEDIATE")?;
            let result = (|| {
                self.identities
                    .iter()
                    .try_for_each(|identity| apply_identity(connection, &vault, identity))?;
                self.validate_filesystem_for_identity()
            })();
            if result.is_ok() {
                connection.execute_batch("COMMIT")?;
            } else {
                let _ = connection.execute_batch("ROLLBACK");
            }
            result
        })?;
        self.phase = Phase::IdentityComplete;
        if let Err(error) = self.persist() {
            // SQLite identity is already committed. The last durable phase is
            // still filesystem_complete, whose recovery path replays the exact
            // frozen row IDs idempotently; never misreport this as pre-commit.
            tracing::warn!(
                result_code = "move_identity_checkpoint_lagging",
                error_code = %error,
                "move identity committed before its journal phase could advance"
            );
        }
        Ok(())
    }

    pub(crate) fn rollback_filesystem(&mut self) -> AppResult<()> {
        self.phase = Phase::RollingBack;
        self.persist()?;
        let vault = self.vault_path()?;
        for (source, _) in &self.directories {
            let source = resolve_vault_path(&vault, source)?;
            if !source.exists() {
                fs::create_dir(&source)?;
                sync_parent_directory(
                    source
                        .parent()
                        .ok_or_else(|| AppError::msg("move directory has no parent"))?,
                )?;
            } else if !source.is_dir() {
                return Err(AppError::msg("move_rollback_source_conflict"));
            }
        }
        for index in (0..self.files.len()).rev() {
            let source = resolve_vault_path(&vault, &self.files[index].from)?;
            let target = resolve_vault_path(&vault, &self.files[index].to)?;
            let witness = self.identity_witness_path(index);
            let expected_hash = self.files[index].hash.clone();
            let source_hash = file_hash_if_regular(&source)?;
            let target_hash = file_hash_if_regular(&target)?;
            match (source_hash, target_hash) {
                (None, Some(hash))
                    if hash == expected_hash && same_file_identity(&target, &witness)? =>
                {
                    self.files[index].state = FileState::Linking;
                    self.persist()?;
                    link_file_no_replace_locked(&target, &source)?;
                    self.files[index].state = FileState::Linked;
                    self.persist()?;
                    if !same_file_identity(&source, &target)? {
                        return Err(AppError::msg("move_rollback_file_conflict"));
                    }
                    remove_file_durable(&target)?;
                    self.files[index].state = FileState::Planned;
                    self.persist()?;
                }
                (Some(source_hash), Some(target_hash))
                    if source_hash == expected_hash
                        && target_hash == expected_hash
                        && matches!(
                            self.files[index].state,
                            FileState::Linking | FileState::Linked
                        )
                        && same_file_identity(&source, &target)?
                        && same_file_identity(&target, &witness)? =>
                {
                    self.files[index].state = FileState::Linked;
                    self.persist()?;
                    remove_file_durable(&target)?;
                    self.files[index].state = FileState::Planned;
                    self.persist()?;
                }
                (Some(hash), None)
                    if hash == expected_hash && same_file_identity(&source, &witness)? =>
                {
                    self.files[index].state = FileState::Planned;
                    self.persist()?;
                }
                _ => return Err(AppError::msg("move_rollback_file_conflict")),
            }
        }
        for (_, target) in self.directories.iter().rev() {
            let target = resolve_vault_path(&vault, target)?;
            if target.exists() {
                fs::remove_dir(&target)
                    .map_err(|_| AppError::msg("move_rollback_target_tree_conflict"))?;
                sync_parent_directory(
                    target
                        .parent()
                        .ok_or_else(|| AppError::msg("move directory has no parent"))?,
                )?;
            }
        }
        self.cleanup_materials()?;
        self.remove_journal()
    }

    pub(crate) fn apply_backlinks(&mut self, state: &AppState) -> BacklinkOutcome {
        let mut outcome = BacklinkOutcome::default();
        for index in 0..self.backlinks.len() {
            if matches!(self.backlinks[index].state, BacklinkState::Published) {
                outcome
                    .applied_paths
                    .push(self.backlinks[index].path.clone());
                continue;
            }
            if matches!(self.backlinks[index].state, BacklinkState::Pending) {
                outcome
                    .pending_paths
                    .push(self.backlinks[index].path.clone());
                continue;
            }
            if let Err(error) = self.apply_backlink(index) {
                tracing::warn!(
                    result_code = "move_backlink_publish_conflict",
                    error_code = %error,
                    "move backlink publishing stopped with a durable partial outcome"
                );
                for suffix in index..self.backlinks.len() {
                    if !matches!(self.backlinks[suffix].state, BacklinkState::Published) {
                        self.backlinks[suffix].state = BacklinkState::Pending;
                    }
                }
                outcome.applied_paths.clear();
                outcome.pending_paths.clear();
                for backlink in &self.backlinks {
                    if backlink.state == BacklinkState::Published {
                        state
                            .storage
                            .write_guard
                            .mark(&backlink.path, &backlink.after_hash);
                        outcome.applied_paths.push(backlink.path.clone());
                    } else {
                        outcome.pending_paths.push(backlink.path.clone());
                    }
                }
                self.phase = Phase::PendingConflict;
                if let Err(error) = self.persist() {
                    tracing::warn!(
                        result_code = "move_backlink_checkpoint_recovery_required",
                        error_code = %error,
                        "partial backlink facts could not be checkpointed"
                    );
                    outcome
                        .recovery_warnings
                        .push("move_backlink_checkpoint_recovery_required".into());
                }
                return outcome;
            }
            let path = self.backlinks[index].path.clone();
            state
                .storage
                .write_guard
                .mark(&path, &self.backlinks[index].after_hash);
            outcome.applied_paths.push(path);
        }
        self.phase = Phase::BacklinksComplete;
        if let Err(error) = self.persist() {
            tracing::warn!(
                result_code = "move_backlink_cleanup_required",
                error_code = %error,
                "published backlinks require checkpoint cleanup"
            );
            outcome
                .recovery_warnings
                .push("move_backlink_cleanup_required".into());
            return outcome;
        }
        if let Err(error) = self.cleanup_materials() {
            tracing::warn!(
                result_code = "move_backlink_cleanup_required",
                error_code = %error,
                "published backlinks require material cleanup"
            );
            outcome
                .recovery_warnings
                .push("move_backlink_cleanup_required".into());
            return outcome;
        }
        if let Err(error) = self.remove_journal() {
            tracing::warn!(
                result_code = "move_backlink_cleanup_required",
                error_code = %error,
                "published backlinks require journal cleanup"
            );
            outcome
                .recovery_warnings
                .push("move_backlink_cleanup_required".into());
        }
        outcome
    }

    fn apply_backlink(&mut self, index: usize) -> AppResult<()> {
        let vault = self.vault_path()?;
        let path = resolve_vault_path(&vault, &self.backlinks[index].path)?;
        let material = self.material_path(index);
        let before_hash = self.backlinks[index].before_hash.clone();
        let after_hash = self.backlinks[index].after_hash.clone();
        let path_hash = file_hash_if_regular(&path)?;
        let material_hash = file_hash_if_regular(&material)?;

        if path_hash.as_deref() == Some(&after_hash) {
            self.backlinks[index].state = BacklinkState::Published;
            self.persist()?;
            if material_hash.as_deref() == Some(&before_hash) {
                remove_file_durable(&material)?;
            }
            return Ok(());
        }

        match (path_hash, material_hash) {
            (Some(current), None) if current == before_hash => {
                self.backlinks[index].state = BacklinkState::LinkingMaterial;
                self.persist()?;
                self.ensure_material_directory(&self.material_directory())?;
                link_file_no_replace_locked(&path, &material)?;
                self.backlinks[index].state = BacklinkState::MaterialLinked;
                self.persist()?;
                if !same_file_identity(&path, &material)? {
                    return Err(AppError::msg("move_backlink_content_conflict"));
                }
                remove_file_durable(&path)?;
                self.backlinks[index].state = BacklinkState::Displaced;
                self.persist()?;
            }
            (Some(current), Some(saved))
                if current == before_hash
                    && saved == before_hash
                    && matches!(
                        self.backlinks[index].state,
                        BacklinkState::LinkingMaterial | BacklinkState::MaterialLinked
                    )
                    && same_file_identity(&path, &material)? =>
            {
                self.backlinks[index].state = BacklinkState::MaterialLinked;
                self.persist()?;
                remove_file_durable(&path)?;
                self.backlinks[index].state = BacklinkState::Displaced;
                self.persist()?;
            }
            (None, Some(saved))
                if saved == before_hash
                    && matches!(
                        self.backlinks[index].state,
                        BacklinkState::MaterialLinked | BacklinkState::Displaced
                    ) =>
            {
                self.backlinks[index].state = BacklinkState::Displaced;
                self.persist()?;
            }
            _ => return Err(AppError::msg("move_backlink_content_conflict")),
        }

        let before = fs::read(&material)?;
        if content_hash(&before) != before_hash {
            restore_material_no_replace(&material, &path)?;
            return Err(AppError::msg("move_backlink_content_conflict"));
        }
        let before =
            String::from_utf8(before).map_err(|_| AppError::msg("move_backlink_not_utf8"))?;
        let mut after = before;
        for (from, to) in &self.mappings {
            after = rewrite_wikilinks(&after, from, to);
        }
        if content_hash(after.as_bytes()) != after_hash {
            restore_material_no_replace(&material, &path)?;
            return Err(AppError::msg("move_backlink_frozen_intent_conflict"));
        }
        if let Err(error) = atomic_create(&path, after.as_bytes()) {
            let _ = restore_material_no_replace(&material, &path);
            return Err(error);
        }
        self.backlinks[index].state = BacklinkState::Published;
        self.persist()?;
        remove_file_durable(&material)?;
        Ok(())
    }

    pub(crate) fn finish_without_backlinks(&mut self) -> AppResult<()> {
        self.phase = Phase::BacklinksComplete;
        self.persist()?;
        self.cleanup_materials()?;
        self.remove_journal()
    }

    fn cleanup_materials(&self) -> AppResult<()> {
        let witness_directory = self.identity_witness_directory();
        if witness_directory.exists() {
            for index in 0..self.files.len() {
                let witness = self.identity_witness_path(index);
                if witness.exists() {
                    remove_file_durable(&witness)?;
                }
            }
            fs::remove_dir(&witness_directory)?;
            sync_parent_directory(
                witness_directory
                    .parent()
                    .ok_or_else(|| AppError::msg("move witness has no parent"))?,
            )?;
        }
        let physical_directory = self.physical_material_directory();
        if physical_directory.exists() {
            for index in 0..self.files.len() {
                let material = self.physical_material_path(index);
                if material.exists() {
                    remove_file_durable(&material)?;
                }
            }
            fs::remove_dir(&physical_directory)?;
            sync_parent_directory(
                physical_directory
                    .parent()
                    .ok_or_else(|| AppError::msg("move material has no parent"))?,
            )?;
        }
        let directory = self.material_directory();
        if !directory.exists() {
            return Ok(());
        }
        for index in 0..self.backlinks.len() {
            let material = self.material_path(index);
            if material.exists() {
                remove_file_durable(&material)?;
            }
        }
        fs::remove_dir(&directory)?;
        sync_parent_directory(
            directory
                .parent()
                .ok_or_else(|| AppError::msg("move material has no parent"))?,
        )
    }

    fn ensure_physical_material(&self, index: usize) -> AppResult<()> {
        let material = self.physical_material_path(index);
        let expected_hash = &self.files[index].hash;
        match file_hash_if_regular(&material)? {
            Some(hash) if &hash == expected_hash => return Ok(()),
            Some(_) => return Err(AppError::msg("move_physical_material_conflict")),
            None => {}
        }
        let vault = self.vault_path()?;
        let source = resolve_vault_path(&vault, &self.files[index].from)?;
        let bytes = fs::read(&source)?;
        if content_hash(&bytes) != *expected_hash {
            return Err(AppError::msg("move_physical_source_conflict"));
        }
        self.ensure_material_directory(&self.physical_material_directory())?;
        atomic_create(&material, &bytes)?;
        if file_hash_if_regular(&material)?.as_deref() != Some(expected_hash) {
            return Err(AppError::msg("move_physical_material_conflict"));
        }
        Ok(())
    }

    fn ensure_identity_witness(&self, index: usize) -> AppResult<()> {
        let vault = self.vault_path()?;
        let source = resolve_vault_path(&vault, &self.files[index].from)?;
        let target = resolve_vault_path(&vault, &self.files[index].to)?;
        let witness = self.identity_witness_path(index);
        let expected_hash = &self.files[index].hash;
        match file_hash_if_regular(&witness)? {
            Some(hash) if &hash == expected_hash => {
                let source_matches = file_hash_if_regular(&source)?.is_some()
                    && same_file_identity(&source, &witness)?;
                let target_matches = file_hash_if_regular(&target)?.is_some()
                    && same_file_identity(&target, &witness)?;
                if source_matches || target_matches {
                    return Ok(());
                }
                return Err(AppError::msg("move_identity_witness_conflict"));
            }
            Some(_) => return Err(AppError::msg("move_identity_witness_conflict")),
            None => {}
        }
        if file_hash_if_regular(&source)?.as_deref() != Some(expected_hash) {
            return Err(AppError::msg("move_physical_source_conflict"));
        }
        self.ensure_material_directory(&self.identity_witness_directory())?;
        link_file_no_replace_locked(&source, &witness)?;
        if !same_file_identity(&source, &witness)? {
            return Err(AppError::msg("move_identity_witness_conflict"));
        }
        Ok(())
    }

    fn ensure_material_directory(&self, directory: &Path) -> AppResult<()> {
        match fs::symlink_metadata(directory) {
            Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
                Ok(())
            }
            Ok(_) => Err(AppError::msg("move_material_directory_conflict")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(directory)?;
                sync_parent_directory(
                    directory
                        .parent()
                        .ok_or_else(|| AppError::msg("move material has no parent"))?,
                )
            }
            Err(error) => Err(error.into()),
        }
    }

    fn validate_filesystem_for_identity(&self) -> AppResult<()> {
        self.validate_target_tree()?;
        let vault = self.vault_path()?;
        if resolve_vault_path(&vault, &self.old_root)?.exists() {
            return Err(AppError::msg("move_filesystem_source_conflict"));
        }
        for (index, file) in self.files.iter().enumerate() {
            let target = resolve_vault_path(&vault, &file.to)?;
            if file_hash_if_regular(&target)?.as_deref() != Some(&file.hash) {
                return Err(AppError::msg("move_filesystem_target_conflict"));
            }
            if file_hash_if_regular(&self.physical_material_path(index))?.as_deref()
                != Some(&file.hash)
            {
                return Err(AppError::msg("move_physical_material_conflict"));
            }
            let witness = self.identity_witness_path(index);
            if file_hash_if_regular(&witness)?.as_deref() != Some(&file.hash)
                || !same_file_identity(&target, &witness)?
            {
                return Err(AppError::msg("move_identity_witness_conflict"));
            }
        }
        Ok(())
    }

    fn vault_path(&self) -> AppResult<PathBuf> {
        let vault = PathBuf::from(&self.vault);
        let canonical = vault.canonicalize()?;
        if canonical.to_string_lossy() != self.vault {
            return Err(AppError::msg("move_journal_vault_identity_conflict"));
        }
        Ok(canonical)
    }

    fn material_directory(&self) -> PathBuf {
        PathBuf::from(&self.vault)
            .join(".iris/operations/moves")
            .join(format!("{}.backlinks", self.operation_id))
    }

    fn physical_material_directory(&self) -> PathBuf {
        PathBuf::from(&self.vault)
            .join(".iris/operations/moves")
            .join(format!("{}.files", self.operation_id))
    }

    fn identity_witness_directory(&self) -> PathBuf {
        PathBuf::from(&self.vault)
            .join(".iris/operations/moves")
            .join(format!("{}.links", self.operation_id))
    }

    fn identity_witness_path(&self, index: usize) -> PathBuf {
        self.identity_witness_directory().join(index.to_string())
    }

    fn physical_material_path(&self, index: usize) -> PathBuf {
        self.physical_material_directory().join(index.to_string())
    }

    fn material_path(&self, index: usize) -> PathBuf {
        self.material_directory().join(format!("{index}.md"))
    }

    fn persist(&self) -> AppResult<()> {
        atomic_write(&self.journal_path, &serde_json::to_vec(self)?)
    }

    fn remove_journal(&self) -> AppResult<()> {
        if self.journal_path.exists() {
            remove_file_durable(&self.journal_path)?;
        }
        Ok(())
    }
}

pub(crate) fn recover_pending(state: &AppState, vault: &Path) -> Vec<String> {
    let directory = journal_directory(vault);
    let Ok(entries) = fs::read_dir(&directory) else {
        return Vec::new();
    };
    let mut unresolved = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let result = (|| -> AppResult<()> {
            let mut checkpoint: MoveCheckpoint = serde_json::from_slice(&fs::read(&path)?)?;
            checkpoint.journal_path = path.clone();
            if checkpoint.schema_version != JOURNAL_VERSION
                || checkpoint.vault_path()? != vault.canonicalize()?
            {
                return Err(AppError::msg("move_journal_scope_conflict"));
            }
            if checkpoint.phase == Phase::RollingBack {
                checkpoint.rollback_filesystem()?;
                return Ok(());
            }
            if matches!(checkpoint.phase, Phase::Prepared | Phase::MovingFilesystem) {
                checkpoint.move_filesystem()?;
            }
            if checkpoint.phase == Phase::FilesystemComplete {
                checkpoint.commit_identity(state)?;
            }
            if checkpoint.phase == Phase::IdentityComplete {
                if checkpoint.backlinks.is_empty() {
                    checkpoint.finish_without_backlinks()?;
                } else {
                    let outcome = checkpoint.apply_backlinks(state);
                    if !outcome.pending_paths.is_empty() {
                        return Err(AppError::msg("move_recovery_backlink_pending"));
                    }
                }
            }
            if checkpoint.phase == Phase::BacklinksComplete {
                checkpoint.cleanup_materials()?;
                checkpoint.remove_journal()?;
            }
            if checkpoint.phase == Phase::PendingConflict {
                return Err(AppError::msg("move_recovery_conflict"));
            }
            Ok(())
        })();
        if let Err(error) = result {
            let journal = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("unknown");
            tracing::warn!(
                result_code = "move_recovery_required",
                journal,
                error_code = %error,
                "a durable move checkpoint requires attention"
            );
            unresolved.push(journal.to_string());
        }
    }
    unresolved
}

fn journal_directory(vault: &Path) -> PathBuf {
    vault.join(".iris/operations/moves")
}

fn collect_physical_items(
    vault: &Path,
    kind: MoveKind,
    old_root: &str,
    new_root: &str,
) -> AppResult<PhysicalInventory> {
    let source = resolve_vault_path(vault, old_root)?;
    let mut files = Vec::new();
    let mut directories = Vec::new();
    match kind {
        MoveKind::File => {
            let bytes = fs::read(&source)?;
            files.push(PhysicalFile {
                from: old_root.into(),
                to: new_root.into(),
                hash: content_hash(&bytes),
                state: FileState::Planned,
            });
        }
        MoveKind::Folder => collect_directory_items(
            vault,
            &source,
            old_root,
            new_root,
            &mut files,
            &mut directories,
        )?,
    }
    directories.sort_by_key(|(from, _)| from.matches('/').count());
    files.sort_by(|left, right| left.from.cmp(&right.from));
    Ok((files, directories))
}

fn collect_directory_items(
    vault: &Path,
    directory: &Path,
    old_root: &str,
    new_root: &str,
    files: &mut Vec<PhysicalFile>,
    directories: &mut Vec<(String, String)>,
) -> AppResult<()> {
    let source_relative = relative_path(vault, directory)?;
    let suffix = source_relative
        .strip_prefix(old_root)
        .ok_or_else(|| AppError::msg("move_source_scope_conflict"))?;
    let target_relative = format!("{new_root}{suffix}");
    directories.push((source_relative, target_relative));
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            return Err(AppError::msg("move_special_filesystem_entry"));
        }
        if metadata.is_dir() {
            collect_directory_items(vault, &entry.path(), old_root, new_root, files, directories)?;
        } else if metadata.is_file() {
            let from = relative_path(vault, &entry.path())?;
            let suffix = from
                .strip_prefix(old_root)
                .ok_or_else(|| AppError::msg("move_source_scope_conflict"))?
                .to_string();
            files.push(PhysicalFile {
                from,
                to: format!("{new_root}{suffix}"),
                hash: content_hash(&fs::read(entry.path())?),
                state: FileState::Planned,
            });
        } else {
            return Err(AppError::msg("move_special_filesystem_entry"));
        }
    }
    Ok(())
}

fn capture_identity(
    connection: &rusqlite::Connection,
    vault: &Path,
    from: &str,
    to: &str,
) -> AppResult<IdentityMove> {
    let file_id = connection
        .query_row("SELECT id FROM files WHERE path = ?1", [from], |row| {
            row.get(0)
        })
        .optional()?;
    let stale_target_file_id = connection
        .query_row("SELECT id FROM files WHERE path = ?1", [to], |row| {
            row.get(0)
        })
        .optional()?;
    let mut statement = connection.prepare(
        "SELECT id FROM versions WHERE vault_path = ?1 AND note_path = ?2
         AND recycle_id IS NULL ORDER BY id",
    )?;
    let version_ids = statement
        .query_map(params![vault.to_string_lossy(), from], |row| row.get(0))?
        .collect::<Result<Vec<i64>, _>>()?;
    Ok(IdentityMove {
        from: from.into(),
        to: to.into(),
        file_id,
        stale_target_file_id,
        version_ids,
    })
}

fn apply_identity(
    connection: &rusqlite::Connection,
    vault: &Path,
    identity: &IdentityMove,
) -> AppResult<()> {
    let current_source_file_id: Option<i64> = connection
        .query_row(
            "SELECT id FROM files WHERE path = ?1",
            [&identity.from],
            |row| row.get(0),
        )
        .optional()?;
    match identity.file_id {
        Some(expected) if current_source_file_id.is_some_and(|id| id != expected) => {
            return Err(AppError::msg("move_file_source_identity_conflict"));
        }
        None if current_source_file_id.is_some() => {
            return Err(AppError::msg("move_file_source_identity_conflict"));
        }
        _ => {}
    }
    let mut statement = connection.prepare(
        "SELECT id FROM versions WHERE vault_path = ?1 AND note_path = ?2
         AND recycle_id IS NULL ORDER BY id",
    )?;
    let current_old_ids: BTreeSet<i64> = statement
        .query_map(params![vault.to_string_lossy(), identity.from], |row| {
            row.get(0)
        })?
        .collect::<Result<_, _>>()?;
    let expected_ids: BTreeSet<_> = identity.version_ids.iter().copied().collect();
    if !current_old_ids.is_subset(&expected_ids) {
        return Err(AppError::msg("move_identity_new_source_version_conflict"));
    }
    let mut target_statement = connection.prepare(
        "SELECT id FROM versions WHERE vault_path = ?1 AND note_path = ?2
         AND recycle_id IS NULL ORDER BY id",
    )?;
    let current_target_ids: BTreeSet<i64> = target_statement
        .query_map(params![vault.to_string_lossy(), identity.to], |row| {
            row.get(0)
        })?
        .collect::<Result<_, _>>()?;
    if !current_target_ids.is_subset(&expected_ids) {
        return Err(AppError::msg("move_identity_destination_version_conflict"));
    }
    for id in &identity.version_ids {
        let path: Option<String> = connection
            .query_row(
                "SELECT note_path FROM versions WHERE id = ?1 AND vault_path = ?2
                 AND recycle_id IS NULL",
                params![id, vault.to_string_lossy()],
                |row| row.get(0),
            )
            .optional()?;
        match path.as_deref() {
            Some(path) if path == identity.from => {
                connection.execute(
                    "UPDATE versions SET note_path = ?1 WHERE id = ?2",
                    params![identity.to, id],
                )?;
            }
            Some(path) if path == identity.to => {}
            _ => return Err(AppError::msg("move_version_identity_conflict")),
        }
    }
    let current_target_file_id: Option<i64> = connection
        .query_row(
            "SELECT id FROM files WHERE path = ?1",
            [&identity.to],
            |row| row.get(0),
        )
        .optional()?;
    if current_target_file_id != identity.file_id
        && current_target_file_id != identity.stale_target_file_id
        && current_target_file_id.is_some()
    {
        return Err(AppError::msg("move_file_destination_identity_conflict"));
    }
    if let Some(stale_id) = identity.stale_target_file_id {
        connection.execute(
            "DELETE FROM files WHERE id = ?1 AND path = ?2",
            params![stale_id, identity.to],
        )?;
    }
    if let Some(file_id) = identity.file_id {
        let current: Option<String> = connection
            .query_row("SELECT path FROM files WHERE id = ?1", [file_id], |row| {
                row.get(0)
            })
            .optional()?;
        match current.as_deref() {
            Some(path) if path == identity.from => {
                let renamed_id = rename_file_index(connection, &identity.from, &identity.to)?;
                if renamed_id != file_id {
                    return Err(AppError::msg("move_file_identity_conflict"));
                }
                connection.execute(
                    "UPDATE files SET content_hash = '' WHERE id = ?1",
                    [file_id],
                )?;
            }
            Some(path) if path == identity.to => {}
            _ => return Err(AppError::msg("move_file_identity_conflict")),
        }
    }
    Ok(())
}

fn file_hash_if_regular(path: &Path) -> AppResult<Option<String>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_file() {
        return Err(AppError::msg("move_recovery_target_conflict"));
    }
    Ok(Some(content_hash(&fs::read(path)?)))
}

fn restore_material_no_replace(material: &Path, target: &Path) -> AppResult<()> {
    if !target.exists() {
        link_file_no_replace_locked(material, target)?;
    }
    Ok(())
}

#[cfg(unix)]
fn same_file_identity(left: &Path, right: &Path) -> AppResult<bool> {
    use std::os::unix::fs::MetadataExt;
    let left = fs::metadata(left)?;
    let right = fs::metadata(right)?;
    Ok(left.dev() == right.dev() && left.ino() == right.ino())
}

#[cfg(windows)]
fn same_file_identity(left: &Path, right: &Path) -> AppResult<bool> {
    use std::os::windows::fs::MetadataExt;
    let left = fs::metadata(left)?;
    let right = fs::metadata(right)?;
    Ok(left.volume_serial_number() == right.volume_serial_number()
        && left.file_index() == right.file_index())
}

#[cfg(not(any(unix, windows)))]
fn same_file_identity(_left: &Path, _right: &Path) -> AppResult<bool> {
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::scan::index_file;

    fn setup() -> (tempfile::TempDir, std::sync::Arc<AppState>, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let vault = directory.path().join("vault");
        fs::create_dir_all(vault.join("old")).unwrap();
        fs::write(vault.join("old/a.md"), "alpha").unwrap();
        fs::write(vault.join("old/b.md"), "beta").unwrap();
        fs::write(vault.join("old/image.bin"), b"original attachment").unwrap();
        let state =
            AppState::new_with_test_cas_key(directory.path().join("data"), [0xA4; 32]).unwrap();
        state.set_vault(vault.clone()).unwrap();
        let vault = state.vault_path().unwrap();
        state
            .db
            .with_conn(|connection| {
                index_file(connection, &vault, &vault.join("old/a.md"))?;
                index_file(connection, &vault, &vault.join("old/b.md"))?;
                Ok(())
            })
            .unwrap();
        (directory, state, vault)
    }

    fn checkpoint(state: &AppState, vault: &Path) -> MoveCheckpoint {
        let mappings = BTreeMap::from([
            ("old/a.md".to_string(), "new/a.md".to_string()),
            ("old/b.md".to_string(), "new/b.md".to_string()),
        ]);
        MoveCheckpoint::create(
            state,
            vault,
            MoveKind::Folder,
            ("old", "new"),
            &mappings,
            &[],
            &[],
        )
        .unwrap()
    }

    fn physical_material(checkpoint: &MoveCheckpoint, source: &str) -> PathBuf {
        let index = checkpoint
            .files
            .iter()
            .position(|item| item.from == source)
            .expect("physical item");
        PathBuf::from(&checkpoint.vault)
            .join(".iris/operations/moves")
            .join(format!("{}.files/{index}", checkpoint.operation_id))
    }

    fn identity_witness(checkpoint: &MoveCheckpoint, source: &str) -> PathBuf {
        let index = checkpoint
            .files
            .iter()
            .position(|item| item.from == source)
            .expect("physical item");
        PathBuf::from(&checkpoint.vault)
            .join(".iris/operations/moves")
            .join(format!("{}.links/{index}", checkpoint.operation_id))
    }

    #[test]
    fn vault_activation_recovers_a_half_moved_directory_from_exact_items() {
        let (_directory, state, vault) = setup();
        let mut checkpoint = checkpoint(&state, &vault);
        checkpoint.phase = Phase::MovingFilesystem;
        checkpoint.persist().unwrap();
        checkpoint.move_file_forward(0).unwrap();
        drop(checkpoint);

        state.set_vault(vault.clone()).unwrap();

        assert!(!vault.join("old").exists());
        assert_eq!(fs::read_to_string(vault.join("new/a.md")).unwrap(), "alpha");
        assert_eq!(fs::read_to_string(vault.join("new/b.md")).unwrap(), "beta");
        state
            .db
            .with_read_conn(|connection| {
                let count: i64 = connection.query_row(
                    "SELECT COUNT(*) FROM files WHERE path IN ('new/a.md', 'new/b.md')",
                    [],
                    |row| row.get(0),
                )?;
                assert_eq!(count, 2);
                Ok(())
            })
            .unwrap();
        assert_eq!(fs::read_dir(journal_directory(&vault)).unwrap().count(), 0);
    }

    #[test]
    fn vault_activation_commits_frozen_identity_after_filesystem_completed() {
        let (_directory, state, vault) = setup();
        let mut checkpoint = checkpoint(&state, &vault);
        checkpoint.move_filesystem().unwrap();
        drop(checkpoint);

        state.set_vault(vault.clone()).unwrap();

        assert!(vault.join("new/a.md").is_file());
        state
            .db
            .with_read_conn(|connection| {
                let old: i64 = connection.query_row(
                    "SELECT COUNT(*) FROM files WHERE path LIKE 'old/%'",
                    [],
                    |row| row.get(0),
                )?;
                let new: i64 = connection.query_row(
                    "SELECT COUNT(*) FROM files WHERE path LIKE 'new/%'",
                    [],
                    |row| row.get(0),
                )?;
                assert_eq!((old, new), (0, 2));
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn recovery_stops_when_a_moved_target_was_edited_and_keeps_checkpoint() {
        let (_directory, state, vault) = setup();
        let mut checkpoint = checkpoint(&state, &vault);
        checkpoint.phase = Phase::MovingFilesystem;
        checkpoint.persist().unwrap();
        checkpoint.move_file_forward(0).unwrap();
        let journal = checkpoint.journal_path.clone();
        fs::write(vault.join("new/a.md"), "external edit").unwrap();
        drop(checkpoint);

        state.set_vault(vault.clone()).unwrap();

        assert_eq!(
            fs::read_to_string(vault.join("new/a.md")).unwrap(),
            "external edit"
        );
        assert!(vault.join("old/b.md").is_file());
        assert!(journal.is_file());
    }

    #[test]
    fn identity_commit_rejects_in_place_target_edit_and_preserves_independent_materials() {
        let (_directory, state, vault) = setup();
        let mut checkpoint = checkpoint(&state, &vault);
        checkpoint.move_filesystem().unwrap();
        let note_material = physical_material(&checkpoint, "old/a.md");
        let attachment_material = physical_material(&checkpoint, "old/image.bin");

        assert!(!same_file_identity(&note_material, &vault.join("new/a.md")).unwrap());
        fs::write(vault.join("new/a.md"), "external in-place edit").unwrap();

        let error = checkpoint
            .commit_identity(&state)
            .expect_err("changed target must not receive frozen identity");
        checkpoint
            .rollback_filesystem()
            .expect_err("rollback must not consume an external in-place edit");

        assert!(error.to_string().contains("filesystem"));
        assert_eq!(fs::read(&note_material).unwrap(), b"alpha");
        assert_eq!(
            fs::read(&attachment_material).unwrap(),
            b"original attachment"
        );
        assert_eq!(
            fs::read_to_string(vault.join("new/a.md")).unwrap(),
            "external in-place edit"
        );
        state
            .db
            .with_read_conn(|connection| {
                let old: i64 = connection.query_row(
                    "SELECT COUNT(*) FROM files WHERE path LIKE 'old/%'",
                    [],
                    |row| row.get(0),
                )?;
                let new: i64 = connection.query_row(
                    "SELECT COUNT(*) FROM files WHERE path LIKE 'new/%'",
                    [],
                    |row| row.get(0),
                )?;
                assert_eq!((old, new), (2, 0));
                Ok(())
            })
            .unwrap();
        assert!(checkpoint.journal_path.is_file());
    }

    #[test]
    fn identity_commit_rejects_replaced_target_and_preserves_independent_materials() {
        let (_directory, state, vault) = setup();
        let mut checkpoint = checkpoint(&state, &vault);
        checkpoint.move_filesystem().unwrap();
        let note_material = physical_material(&checkpoint, "old/a.md");
        let attachment_material = physical_material(&checkpoint, "old/image.bin");
        fs::remove_file(vault.join("new/a.md")).unwrap();
        fs::write(vault.join("new/a.md"), "alpha").unwrap();

        assert!(!same_file_identity(
            &identity_witness(&checkpoint, "old/a.md"),
            &vault.join("new/a.md")
        )
        .unwrap());

        checkpoint
            .commit_identity(&state)
            .expect_err("replaced target must not receive frozen identity");
        checkpoint
            .rollback_filesystem()
            .expect_err("rollback must not consume an external same-hash replacement");

        assert_eq!(fs::read(&note_material).unwrap(), b"alpha");
        assert_eq!(
            fs::read(&attachment_material).unwrap(),
            b"original attachment"
        );
        assert_eq!(fs::read_to_string(vault.join("new/a.md")).unwrap(), "alpha");
        assert!(checkpoint.journal_path.is_file());
    }

    #[test]
    fn identity_commit_rejects_a_source_index_created_after_none_was_frozen() {
        let (_directory, state, vault) = setup();
        state
            .db
            .with_conn(|connection| {
                connection.execute("DELETE FROM files WHERE path = 'old/b.md'", [])?;
                Ok(())
            })
            .unwrap();
        let mut checkpoint = checkpoint(&state, &vault);
        state
            .db
            .with_conn(|connection| {
                index_file(connection, &vault, &vault.join("old/b.md"))?;
                Ok(())
            })
            .unwrap();
        checkpoint.move_filesystem().unwrap();

        let error = checkpoint
            .commit_identity(&state)
            .expect_err("new source identity must conflict with frozen None");

        assert!(error.to_string().contains("source_identity"));
        state
            .db
            .with_read_conn(|connection| {
                let old: i64 = connection.query_row(
                    "SELECT COUNT(*) FROM files WHERE path = 'old/b.md'",
                    [],
                    |row| row.get(0),
                )?;
                let new: i64 = connection.query_row(
                    "SELECT COUNT(*) FROM files WHERE path = 'new/b.md'",
                    [],
                    |row| row.get(0),
                )?;
                let old_fts: i64 = connection.query_row(
                    "SELECT COUNT(*) FROM files_fts WHERE path = 'old/b.md'",
                    [],
                    |row| row.get(0),
                )?;
                assert_eq!((old, new, old_fts), (1, 0, 1));
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn backlink_compare_and_publish_preserves_an_external_edit_as_pending() {
        let directory = tempfile::tempdir().unwrap();
        let vault = directory.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        fs::write(vault.join("old.md"), "old note").unwrap();
        fs::write(vault.join("ref.md"), "See [[old.md]]").unwrap();
        let state =
            AppState::new_with_test_cas_key(directory.path().join("data"), [0xA4; 32]).unwrap();
        state.set_vault(vault.clone()).unwrap();
        let vault = state.vault_path().unwrap();
        state
            .db
            .with_conn(|connection| {
                index_file(connection, &vault, &vault.join("old.md"))?;
                index_file(connection, &vault, &vault.join("ref.md"))?;
                Ok(())
            })
            .unwrap();
        let recovery =
            crate::storage::note_operations::protect_snapshot(&state, "ref.md", "See [[old.md]]")
                .unwrap();
        let mappings = BTreeMap::from([("old.md".to_string(), "new.md".to_string())]);
        let edits = vec![BacklinkEdit {
            path: "ref.md".into(),
            before: "See [[old.md]]".into(),
            after: "See [[new.md]]".into(),
        }];
        let mut checkpoint = MoveCheckpoint::create(
            &state,
            &vault,
            MoveKind::File,
            ("old.md", "new.md"),
            &mappings,
            &edits,
            &[("ref.md".into(), recovery)],
        )
        .unwrap();
        let journal_json = fs::read_to_string(&checkpoint.journal_path).unwrap();
        assert!(!journal_json.contains("See [[old.md]]"));
        assert!(!journal_json.contains("See [[new.md]]"));
        checkpoint.move_filesystem().unwrap();
        checkpoint.commit_identity(&state).unwrap();
        fs::write(vault.join("ref.md"), "external edit").unwrap();

        let outcome = checkpoint.apply_backlinks(&state);

        assert_eq!(outcome.pending_paths, vec!["ref.md"]);
        assert_eq!(
            fs::read_to_string(vault.join("ref.md")).unwrap(),
            "external edit"
        );
        assert!(checkpoint.journal_path.is_file());
    }

    #[test]
    fn backlink_cleanup_failure_preserves_the_published_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let vault = directory.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        fs::write(vault.join("old.md"), "old note").unwrap();
        fs::write(vault.join("ref.md"), "See [[old.md]]").unwrap();
        let state =
            AppState::new_with_test_cas_key(directory.path().join("data"), [0xA4; 32]).unwrap();
        state.set_vault(vault.clone()).unwrap();
        let vault = state.vault_path().unwrap();
        state
            .db
            .with_conn(|connection| {
                index_file(connection, &vault, &vault.join("old.md"))?;
                index_file(connection, &vault, &vault.join("ref.md"))?;
                Ok(())
            })
            .unwrap();
        let mappings = BTreeMap::from([("old.md".to_string(), "new.md".to_string())]);
        let backlinks = vec![BacklinkEdit {
            path: "ref.md".into(),
            before: "See [[old.md]]".into(),
            after: "See [[new.md]]".into(),
        }];
        let recovery_version =
            crate::storage::note_operations::protect_snapshot(&state, "ref.md", "See [[old.md]]")
                .unwrap();
        let mut checkpoint = MoveCheckpoint::create(
            &state,
            &vault,
            MoveKind::File,
            ("old.md", "new.md"),
            &mappings,
            &backlinks,
            &[("ref.md".into(), recovery_version)],
        )
        .unwrap();
        checkpoint.move_filesystem().unwrap();
        checkpoint.commit_identity(&state).unwrap();
        fs::create_dir_all(checkpoint.material_directory()).unwrap();
        fs::write(
            checkpoint.material_directory().join("unexpected"),
            "block cleanup",
        )
        .unwrap();

        let outcome = checkpoint.apply_backlinks(&state);

        assert_eq!(outcome.applied_paths, vec!["ref.md"]);
        assert!(outcome.pending_paths.is_empty());
        assert_eq!(
            outcome.recovery_warnings,
            vec!["move_backlink_cleanup_required"]
        );
        assert_eq!(
            fs::read_to_string(vault.join("ref.md")).unwrap(),
            "See [[new.md]]"
        );
        assert!(checkpoint.journal_path.is_file());
    }
}
