use crate::{
    api::{
        ApiClient, ApiError, CreateModelComponent, CreateModelConnection, FolderEntry,
        OrganismComponent, OrganismModel,
    },
    config::Language,
    i18n::text,
    state::Notice,
    theme,
};
use eframe::egui;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    sync::mpsc::{self, Receiver, Sender},
};
use tokio::runtime::Runtime;
use uuid::Uuid;

pub enum CreaturesPageEvent {
    SessionExpired,
    Notice(Notice),
}

enum RequestResult {
    Models {
        generation: u64,
        result: Result<(Vec<OrganismModel>, Vec<OrganismComponent>), ApiError>,
    },
    Folder {
        generation: u64,
        model_id: String,
        path: String,
        result: Result<Vec<FolderEntry>, ApiError>,
    },
    Deleted {
        generation: u64,
        result: Result<(), ApiError>,
    },
    Catalog {
        generation: u64,
        result: Result<(Vec<OrganismComponent>, Vec<OrganismComponent>), ApiError>,
    },
    Created {
        generation: u64,
        result: Result<OrganismModel, ApiError>,
    },
    FileLoaded {
        generation: u64,
        model_id: String,
        path: String,
        result: Result<String, ApiError>,
    },
    FilesChanged {
        generation: u64,
        model_id: String,
        folder_path: String,
        notice_key: &'static str,
        result: Result<Vec<FolderEntry>, ApiError>,
    },
}

#[derive(Clone)]
struct DraftComponent {
    id: String,
    component_id: String,
    name: String,
    category: String,
    slots: std::collections::BTreeMap<String, u32>,
    position: egui::Vec2,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NewEntryKind {
    File,
    Folder,
}

pub struct CreaturesPage {
    models: Vec<OrganismModel>,
    selected_id: Option<String>,
    folder_path: String,
    folder_entries: Vec<FolderEntry>,
    editor_path: String,
    editor_content: String,
    editor_open: bool,
    new_folder_name: String,
    new_entry_name: String,
    new_entry_kind: Option<NewEntryKind>,
    delete_entry_target: Option<String>,
    selected_entry: Option<String>,
    rename_name: String,
    initialized: bool,
    pending: bool,
    delete_confirmed: bool,
    building: bool,
    catalog: Vec<OrganismComponent>,
    unlocked_ids: BTreeSet<String>,
    draft_name: String,
    draft_components: Vec<DraftComponent>,
    draft_connections: Vec<CreateModelConnection>,
    selected_instance: Option<String>,
    connection_error: Option<&'static str>,
    creation_error: Option<&'static str>,
    generation: u64,
    sender: Sender<RequestResult>,
    receiver: Receiver<RequestResult>,
}

impl CreaturesPage {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            models: Vec::new(),
            selected_id: None,
            folder_path: String::new(),
            folder_entries: Vec::new(),
            editor_path: String::new(),
            editor_content: String::new(),
            editor_open: false,
            new_folder_name: String::new(),
            new_entry_name: String::new(),
            new_entry_kind: None,
            delete_entry_target: None,
            selected_entry: None,
            rename_name: String::new(),
            initialized: false,
            pending: false,
            delete_confirmed: false,
            building: false,
            catalog: Vec::new(),
            unlocked_ids: BTreeSet::new(),
            draft_name: String::new(),
            draft_components: Vec::new(),
            draft_connections: Vec::new(),
            selected_instance: None,
            connection_error: None,
            creation_error: None,
            generation: 0,
            sender,
            receiver,
        }
    }

    pub fn reset(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.models.clear();
        self.selected_id = None;
        self.folder_entries.clear();
        self.folder_path.clear();
        self.clear_file_workspace();
        self.initialized = false;
        self.pending = false;
        self.delete_confirmed = false;
        self.building = false;
        self.catalog.clear();
        self.unlocked_ids.clear();
        self.clear_draft();
    }

    pub fn is_busy(&self) -> bool {
        self.pending
    }

    pub fn poll(&mut self, language: Language) -> Option<CreaturesPageEvent> {
        let result = self.receiver.try_recv().ok()?;
        let generation = match &result {
            RequestResult::Models { generation, .. }
            | RequestResult::Folder { generation, .. }
            | RequestResult::Deleted { generation, .. }
            | RequestResult::Catalog { generation, .. }
            | RequestResult::Created { generation, .. }
            | RequestResult::FileLoaded { generation, .. }
            | RequestResult::FilesChanged { generation, .. } => *generation,
        };
        if generation != self.generation {
            return None;
        }
        self.pending = false;
        match result {
            RequestResult::Models {
                result: Ok((models, catalog)),
                ..
            } => {
                self.models = models;
                self.catalog = catalog;
                self.initialized = true;
                if self
                    .selected_id
                    .as_ref()
                    .is_some_and(|selected| !self.models.iter().any(|model| &model.id == selected))
                {
                    self.selected_id = None;
                }
                None
            }
            RequestResult::Folder {
                model_id,
                path,
                result: Ok(entries),
                ..
            } => {
                if self.selected_id.as_deref() == Some(model_id.as_str()) {
                    self.folder_path = path;
                    self.folder_entries = entries;
                    self.selected_entry = None;
                    self.rename_name.clear();
                    self.delete_entry_target = None;
                }
                None
            }
            RequestResult::Deleted { result: Ok(()), .. } => {
                self.selected_id = None;
                self.folder_path.clear();
                self.folder_entries.clear();
                self.initialized = false;
                self.delete_confirmed = false;
                Some(CreaturesPageEvent::Notice(Notice::success(text(
                    language,
                    "creatures.deleted",
                ))))
            }
            RequestResult::Catalog {
                result: Ok((catalog, unlocked)),
                ..
            } => {
                self.catalog = catalog;
                self.unlocked_ids = unlocked.into_iter().map(|component| component.id).collect();
                None
            }
            RequestResult::Created {
                result: Ok(model), ..
            } => {
                self.building = false;
                self.clear_draft();
                self.models.push(model);
                self.models
                    .sort_by(|first, second| second.updated_at.cmp(&first.updated_at));
                Some(CreaturesPageEvent::Notice(Notice::success(text(
                    language,
                    "creatures.created",
                ))))
            }
            RequestResult::FileLoaded {
                model_id,
                path,
                result: Ok(content),
                ..
            } => {
                if self.selected_id.as_deref() == Some(model_id.as_str()) {
                    self.editor_path = path;
                    self.editor_content = content;
                    self.editor_open = true;
                }
                None
            }
            RequestResult::FilesChanged {
                model_id,
                folder_path,
                notice_key,
                result: Ok(entries),
                ..
            } => {
                if self.selected_id.as_deref() == Some(model_id.as_str())
                    && self.folder_path == folder_path
                {
                    self.folder_entries = entries;
                    self.new_folder_name.clear();
                    self.delete_entry_target = None;
                    self.selected_entry = None;
                    self.rename_name.clear();
                }
                Some(CreaturesPageEvent::Notice(Notice::success(text(
                    language, notice_key,
                ))))
            }
            RequestResult::Models {
                result: Err(error), ..
            }
            | RequestResult::Folder {
                result: Err(error), ..
            }
            | RequestResult::Deleted {
                result: Err(error), ..
            }
            | RequestResult::Catalog {
                result: Err(error), ..
            }
            | RequestResult::Created {
                result: Err(error), ..
            }
            | RequestResult::FileLoaded {
                result: Err(error), ..
            }
            | RequestResult::FilesChanged {
                result: Err(error), ..
            } => {
                if error.is_unauthorized() {
                    Some(CreaturesPageEvent::SessionExpired)
                } else {
                    Some(CreaturesPageEvent::Notice(Notice::from_api_error(
                        language, error,
                    )))
                }
            }
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        language: Language,
        token: &str,
        client: &ApiClient,
        runtime: &Runtime,
    ) {
        if !self.initialized && !self.pending {
            self.start_load(token, client, runtime);
        }
        if self.building {
            self.show_builder(ui, language, token, client, runtime);
        } else if let Some(model) = self
            .selected_id
            .as_ref()
            .and_then(|id| self.models.iter().find(|model| &model.id == id))
            .cloned()
        {
            self.show_detail(ui, language, token, client, runtime, &model);
        } else {
            self.show_list(ui, language, token, client, runtime);
        }
    }

    fn show_list(
        &mut self,
        ui: &mut egui::Ui,
        language: Language,
        token: &str,
        client: &ApiClient,
        runtime: &Runtime,
    ) {
        ui.horizontal(|ui| {
            ui.heading(text(language, "creatures.title"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !self.pending,
                        egui::Button::new(text(language, "common.refresh")),
                    )
                    .clicked()
                {
                    self.start_load(token, client, runtime);
                }
            });
        });
        ui.label(text(language, "creatures.description"));
        ui.add_space(18.0);
        if self.pending && self.models.is_empty() {
            ui.spinner();
            ui.label(text(language, "api.loading"));
            return;
        }
        if self.models.is_empty() {
            egui::Frame::default()
                .fill(theme::SURFACE)
                .corner_radius(8)
                .inner_margin(24.0)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.strong(text(language, "creatures.empty"));
                    ui.weak(text(language, "creatures.empty_hint"));
                });
            ui.add_space(10.0);
        }

        let mut selected = None;
        egui::ScrollArea::vertical()
            .id_salt("organism_model_list")
            .show(ui, |ui| {
                for model in &self.models {
                    let response = egui::Frame::default()
                        .fill(theme::SURFACE)
                        .stroke(egui::Stroke::new(1.0, theme::GRID))
                        .corner_radius(8)
                        .inner_margin(16.0)
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.strong(&model.name);
                                    ui.weak(format!(
                                        "{} · {} {}",
                                        short_id(&model.id),
                                        model.composition.components.len(),
                                        text(language, "creatures.components")
                                    ));
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(">");
                                        ui.weak(&model.updated_at);
                                    },
                                );
                            });
                        })
                        .response
                        .interact(egui::Sense::click());
                    if response.clicked() {
                        selected = Some(model.id.clone());
                    }
                    ui.add_space(8.0);
                }
                ui.separator();
                if ui
                    .add_enabled(
                        !self.pending,
                        egui::Button::new(format!("+ {}", text(language, "creatures.add")))
                            .min_size(egui::vec2(ui.available_width(), 42.0)),
                    )
                    .clicked()
                {
                    self.open_builder(token, client, runtime);
                }
            });
        if let Some(model_id) = selected {
            self.selected_id = Some(model_id.clone());
            self.folder_path.clear();
            self.folder_entries.clear();
            self.clear_file_workspace();
            self.delete_confirmed = false;
            self.start_folder(token, client, runtime, model_id, String::new());
        }
    }

    fn show_builder(
        &mut self,
        ui: &mut egui::Ui,
        language: Language,
        token: &str,
        client: &ApiClient,
        runtime: &Runtime,
    ) {
        let mut cancel = false;
        ui.horizontal(|ui| {
            if ui.button(text(language, "common.back")).clicked() {
                cancel = true;
            }
            ui.heading(text(language, "creatures.builder_title"));
        });
        if cancel {
            self.building = false;
            self.clear_draft();
            return;
        }

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(text(language, "creatures.model_name"));
            if ui
                .add_sized(
                    [360.0_f32.min(ui.available_width()), 34.0],
                    egui::TextEdit::singleline(&mut self.draft_name)
                        .hint_text(text(language, "creatures.model_name_hint")),
                )
                .changed()
            {
                self.creation_error = None;
            }
        });
        ui.add_space(12.0);

        if self.pending && self.catalog.is_empty() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(text(language, "api.loading"));
            });
        }

        let height = (ui.available_height() - 62.0).max(280.0);
        let mut add_component = None;
        let mut clicked_instance = None;
        let mut remove_instance = None;
        ui.columns(2, |columns| {
            columns[0].set_min_height(height);
            columns[1].set_min_height(height);

            egui::Frame::default()
                .fill(theme::SURFACE)
                .stroke(egui::Stroke::new(1.0, theme::GRID))
                .corner_radius(8)
                .inner_margin(14.0)
                .show(&mut columns[0], |ui| {
                    ui.strong(text(language, "creatures.catalog"));
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("organism_component_catalog")
                        .show(ui, |ui| {
                            for component in &self.catalog {
                                let unlocked = self.unlocked_ids.contains(&component.id);
                                let can_add = unlocked && self.can_add_component(component);
                                let stroke = if unlocked {
                                    egui::Stroke::new(1.0, theme::ACCENT)
                                } else {
                                    egui::Stroke::new(1.0, theme::GRID)
                                };
                                let fill = if unlocked {
                                    theme::BACKGROUND
                                } else {
                                    theme::BACKGROUND.gamma_multiply(0.45)
                                };
                                egui::Frame::default()
                                    .fill(fill)
                                    .stroke(stroke)
                                    .corner_radius(6)
                                    .inner_margin(10.0)
                                    .show(ui, |ui| {
                                        ui.set_min_width(ui.available_width());
                                        ui.add_enabled_ui(unlocked, |ui| {
                                            ui.horizontal(|ui| {
                                                ui.vertical(|ui| {
                                                    ui.strong(&component.name);
                                                    ui.weak(format!(
                                                        "{} · {}",
                                                        component.id,
                                                        category_label(
                                                            language,
                                                            &component.category
                                                        )
                                                    ));
                                                    ui.small(slot_summary(language, component));
                                                });
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        if ui
                                                            .add_enabled(
                                                                can_add,
                                                                egui::Button::new(text(
                                                                    language,
                                                                    "creatures.add_component",
                                                                )),
                                                            )
                                                            .clicked()
                                                        {
                                                            add_component = Some(component.clone());
                                                        }
                                                        ui.label(if unlocked {
                                                            text(language, "creatures.unlocked")
                                                        } else {
                                                            text(language, "creatures.locked")
                                                        });
                                                    },
                                                );
                                            });
                                        });
                                    });
                                ui.add_space(7.0);
                            }
                        });
                });

            egui::Frame::default()
                .fill(theme::SURFACE)
                .stroke(egui::Stroke::new(1.0, theme::GRID))
                .corner_radius(8)
                .inner_margin(14.0)
                .show(&mut columns[1], |ui| {
                    ui.strong(text(language, "creatures.draft_structure"));
                    ui.weak(text(language, "creatures.select_connection"));
                    ui.separator();
                    let canvas_height = (height - 105.0).max(190.0);
                    let (canvas_rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), canvas_height),
                        egui::Sense::hover(),
                    );
                    let actions = draw_draft_graph(
                        ui,
                        canvas_rect,
                        &mut self.draft_components,
                        &self.draft_connections,
                        self.selected_instance.as_deref(),
                        language,
                    );
                    clicked_instance = actions.clicked;
                    remove_instance = actions.removed;
                    if self.selected_instance.is_some() {
                        ui.colored_label(
                            theme::ACCENT,
                            text(language, "creatures.connection_source"),
                        );
                    }
                });
        });

        if let Some(component) = add_component {
            if self.can_add_component(&component) {
                self.creation_error = None;
                let position = next_node_position(self.draft_components.len());
                let draft = DraftComponent {
                    id: Uuid::new_v4().to_string(),
                    component_id: component.id,
                    name: component.name,
                    category: component.category,
                    slots: component.slots.counts,
                    position,
                };
                if draft.category == "body"
                    && !self
                        .draft_components
                        .iter()
                        .any(|component| component.category == "body")
                {
                    self.draft_components.insert(0, draft);
                } else {
                    self.draft_components.push(draft);
                }
            }
        }
        if let Some(id) = remove_instance {
            self.creation_error = None;
            self.remove_draft_component(&id);
        } else if let Some(id) = clicked_instance {
            self.creation_error = None;
            self.select_connection_endpoint(id);
        }

        ui.add_space(10.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(text(language, "creatures.create")).clicked() {
                if let Some(error_key) = self.creation_issue() {
                    self.creation_error = Some(error_key);
                } else {
                    self.creation_error = None;
                    self.start_create(token, client, runtime);
                }
            }
            if ui.button(text(language, "common.cancel")).clicked() {
                self.building = false;
                self.clear_draft();
            }
        });
        if !self.draft_components.is_empty() && !self.is_draft_connected() {
            ui.colored_label(
                theme::WARNING,
                text(language, "creatures.graph_disconnected"),
            );
        }
        if let Some(error_key) = self.connection_error {
            ui.colored_label(theme::DANGER, text(language, error_key));
        }
        if let Some(error_key) = self.creation_error {
            ui.colored_label(theme::DANGER, text(language, error_key));
        }
    }

    fn show_detail(
        &mut self,
        ui: &mut egui::Ui,
        language: Language,
        token: &str,
        client: &ApiClient,
        runtime: &Runtime,
        model: &OrganismModel,
    ) {
        ui.horizontal(|ui| {
            if ui
                .button("‹")
                .on_hover_text(text(language, "common.back"))
                .clicked()
            {
                self.selected_id = None;
                self.clear_file_workspace();
            }
            ui.separator();
            ui.vertical(|ui| {
                ui.heading(&model.name);
                ui.weak(format!(
                    "{} · {} components",
                    short_id(&model.id),
                    model.composition.components.len()
                ));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(!self.pending, egui::Button::new("删除"))
                    .clicked()
                {
                    self.delete_confirmed = true;
                }
            });
        });
        ui.add_space(12.0);
        egui::Frame::default()
            .fill(theme::SURFACE)
            .stroke(egui::Stroke::new(1.0, theme::GRID))
            .corner_radius(6)
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.weak("模型详情");
                    ui.separator();
                    ui.label(format!("{} 个组件", model.composition.components.len()));
                    ui.separator();
                    ui.label(format!("{} 个连接", model.composition.connections.len()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.weak(format!("更新于 {}", model.updated_at));
                    });
                });
            });
        ui.add_space(10.0);
        let height = ui.available_height().max(320.0);
        ui.columns(2, |columns| {
            columns[0].set_min_height(height);
            columns[1].set_min_height(height);
            structure_panel(&mut columns[0], language, model, &self.catalog);
            self.folder_panel(&mut columns[1], language, token, client, runtime, model);
        });
        self.file_editor_window(ui.ctx(), language, token, client, runtime, model);
        self.new_entry_window(ui.ctx(), language, token, client, runtime, model);
        self.entry_action_windows(ui.ctx(), language, token, client, runtime, model);
        self.model_delete_window(ui.ctx(), language, token, client, runtime, model);
    }

    fn folder_panel(
        &mut self,
        ui: &mut egui::Ui,
        language: Language,
        token: &str,
        client: &ApiClient,
        runtime: &Runtime,
        model: &OrganismModel,
    ) {
        egui::Frame::default()
            .fill(theme::SURFACE)
            .stroke(egui::Stroke::new(1.0, theme::GRID))
            .corner_radius(8)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_min_height(ui.available_height());
                ui.horizontal(|ui| {
                    ui.strong(text(language, "creatures.files"));
                    ui.weak(format!("{} items", self.folder_entries.len()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.pending {
                            ui.spinner();
                        }
                    });
                });
                ui.add_space(8.0);
                let selected = self
                    .selected_entry
                    .as_ref()
                    .and_then(|path| self.folder_entries.iter().find(|entry| &entry.path == path))
                    .cloned();
                let mut open_selected = false;
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(
                            !self.pending && !self.folder_path.is_empty(),
                            egui::Button::new("↑"),
                        )
                        .on_hover_text(text(language, "creatures.up"))
                        .clicked()
                    {
                        let parent = self
                            .folder_path
                            .rsplit_once('/')
                            .map(|(parent, _)| parent)
                            .unwrap_or("")
                            .to_string();
                        self.start_folder(token, client, runtime, model.id.clone(), parent);
                    }
                    if ui
                        .add_enabled(!self.pending, egui::Button::new("↻"))
                        .on_hover_text(text(language, "common.refresh"))
                        .clicked()
                    {
                        self.start_folder(
                            token,
                            client,
                            runtime,
                            model.id.clone(),
                            self.folder_path.clone(),
                        );
                    }
                    if ui
                        .add_enabled(!self.pending, egui::Button::new("＋ 文件"))
                        .clicked()
                    {
                        self.new_entry_name.clear();
                        self.new_entry_kind = Some(NewEntryKind::File);
                    }
                    if ui
                        .add_enabled(!self.pending, egui::Button::new("＋ 文件夹"))
                        .clicked()
                    {
                        self.new_entry_name.clear();
                        self.new_entry_kind = Some(NewEntryKind::Folder);
                    }
                    if ui
                        .add_enabled(!self.pending, egui::Button::new("导入文件"))
                        .clicked()
                    {
                        if let Some(path) = rfd::FileDialog::new().pick_file() {
                            if let Ok(content) = fs::read(&path) {
                                let name = path
                                    .file_name()
                                    .and_then(|name| name.to_str())
                                    .unwrap_or("imported-file");
                                self.start_import_files(
                                    token,
                                    client,
                                    runtime,
                                    model.id.clone(),
                                    Vec::new(),
                                    vec![(join_model_path(&self.folder_path, name), content)],
                                );
                            }
                        }
                    }
                    if ui
                        .add_enabled(!self.pending, egui::Button::new("导入文件夹"))
                        .clicked()
                    {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            let folder_name = path
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("imported-folder");
                            let destination = join_model_path(&self.folder_path, folder_name);
                            let (folders, files) = collect_import_entries(&path, &destination);
                            if !folders.is_empty() || !files.is_empty() {
                                self.start_import_files(
                                    token,
                                    client,
                                    runtime,
                                    model.id.clone(),
                                    folders,
                                    files,
                                );
                            }
                        }
                    }
                    if ui
                        .add_enabled(
                            !self.pending && selected.is_some(),
                            egui::Button::new("打开"),
                        )
                        .clicked()
                    {
                        open_selected = true;
                    }
                });

                ui.add_space(6.0);
                egui::Frame::default()
                    .fill(theme::BACKGROUND)
                    .corner_radius(5)
                    .inner_margin(egui::Margin::symmetric(9, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.weak("模型");
                            ui.label("/");
                            ui.monospace(display_folder_path(&self.folder_path));
                        });
                    });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.weak("名称");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.weak("大小");
                    });
                });
                ui.separator();

                if self.pending && self.folder_entries.is_empty() {
                    ui.spinner();
                }
                let mut open_folder = None;
                let mut load_file = None;
                egui::ScrollArea::vertical()
                    .id_salt("organism_model_folder")
                    .max_height((ui.available_height() - 105.0).max(180.0))
                    .show(ui, |ui| {
                        for entry in &self.folder_entries {
                            let is_folder = entry.kind == "folder";
                            ui.push_id(&entry.path, |ui| {
                                let selected =
                                    self.selected_entry.as_deref() == Some(entry.path.as_str());
                                let response = egui::Frame::default()
                                    .fill(if selected {
                                        theme::ACCENT.gamma_multiply(0.16)
                                    } else {
                                        egui::Color32::TRANSPARENT
                                    })
                                    .corner_radius(4)
                                    .inner_margin(egui::Margin::symmetric(8, 6))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.horizontal(|ui| {
                                            ui.label(if is_folder { "▾" } else { "·" });
                                            ui.label(&entry.name);
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if !is_folder {
                                                        ui.weak(format_size(
                                                            entry.size.unwrap_or(0),
                                                        ));
                                                    }
                                                },
                                            );
                                        });
                                    })
                                    .response
                                    .interact(egui::Sense::click());
                                if response.clicked() {
                                    self.selected_entry = Some(entry.path.clone());
                                    self.rename_name.clear();
                                }
                                if response.double_clicked() {
                                    if is_folder {
                                        open_folder = Some(entry.path.clone());
                                    } else {
                                        load_file = Some(entry.path.clone());
                                    }
                                }
                                response.context_menu(|ui| {
                                    self.selected_entry = Some(entry.path.clone());
                                    if ui.button(text(language, "creatures.open")).clicked() {
                                        if is_folder {
                                            open_folder = Some(entry.path.clone());
                                        } else {
                                            load_file = Some(entry.path.clone());
                                        }
                                        ui.close();
                                    }
                                    if ui.button(text(language, "creatures.rename")).clicked() {
                                        self.rename_name = entry.name.clone();
                                        ui.close();
                                    }
                                    if ui
                                        .button(text(language, "creatures.delete_entry"))
                                        .clicked()
                                    {
                                        self.delete_entry_target = Some(entry.path.clone());
                                        ui.close();
                                    }
                                });
                            });
                            ui.add_space(2.0);
                        }
                    });
                if open_selected {
                    if let Some(entry) = selected {
                        if entry.kind == "folder" {
                            open_folder = Some(entry.path);
                        } else {
                            load_file = Some(entry.path);
                        }
                    }
                }
                if let Some(path) = open_folder {
                    self.start_folder(token, client, runtime, model.id.clone(), path);
                }
                if let Some(path) = load_file {
                    self.start_load_file(token, client, runtime, model.id.clone(), path);
                }
            });
    }

    fn file_editor_window(
        &mut self,
        ctx: &egui::Context,
        language: Language,
        token: &str,
        client: &ApiClient,
        runtime: &Runtime,
        model: &OrganismModel,
    ) {
        if !self.editor_open {
            return;
        }

        let mut open = self.editor_open;
        let mut save = false;
        let title = if self.editor_path.is_empty() {
            text(language, "creatures.text_editor").to_string()
        } else {
            format!(
                "{} - {}",
                text(language, "creatures.text_editor"),
                self.editor_path
            )
        };
        egui::Window::new(title)
            .id(egui::Id::new("organism_model_text_editor"))
            .open(&mut open)
            .resizable(true)
            // Start centered, but do not anchor the window: anchored windows cannot be dragged.
            .default_pos(ctx.available_rect().center() - egui::vec2(260.0, 190.0))
            .default_size([760.0, 520.0])
            .min_size([480.0, 320.0])
            .max_size([1100.0, 820.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.weak("文件");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.editor_path)
                            .hint_text("src/main.rs")
                            .desired_width(f32::INFINITY),
                    );
                    if ui
                        .add_enabled(
                            !self.pending && !self.editor_path.trim().is_empty(),
                            egui::Button::new("保存"),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.weak("编辑器");
                    ui.weak("纯文本");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.weak(format!("{} 行", self.editor_content.lines().count()));
                    });
                });
                ui.separator();
                ui.add_sized(
                    [ui.available_width(), ui.available_height()],
                    egui::TextEdit::multiline(&mut self.editor_content)
                        .code_editor()
                        .hint_text(text(language, "creatures.file_content")),
                );
            });
        self.editor_open = open;
        if save {
            self.start_save_file(token, client, runtime, model.id.clone());
        }
    }

    fn new_entry_window(
        &mut self,
        ctx: &egui::Context,
        language: Language,
        token: &str,
        client: &ApiClient,
        runtime: &Runtime,
        model: &OrganismModel,
    ) {
        let Some(kind) = self.new_entry_kind else {
            return;
        };
        let title = match kind {
            NewEntryKind::File => text(language, "creatures.new_file"),
            NewEntryKind::Folder => text(language, "creatures.create_folder"),
        };
        let mut open = true;
        let mut confirm = false;
        let mut close_requested = false;
        egui::Window::new(title)
            .id(egui::Id::new("organism_model_new_entry"))
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(text(language, "creatures.entry_name"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_entry_name)
                        .desired_width(280.0)
                        .hint_text(text(language, "creatures.entry_name_hint")),
                );
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.pending && !self.new_entry_name.trim().is_empty(),
                            egui::Button::new(text(language, "creatures.confirm")),
                        )
                        .clicked()
                    {
                        confirm = true;
                    }
                    if ui.button(text(language, "common.cancel")).clicked() {
                        close_requested = true;
                    }
                });
            });
        if confirm {
            let name = self.new_entry_name.trim().to_string();
            let path = join_model_path(&self.folder_path, &name);
            match kind {
                NewEntryKind::Folder => {
                    self.start_create_folder(token, client, runtime, model.id.clone(), path);
                }
                NewEntryKind::File => {
                    self.editor_path = path;
                    self.editor_content.clear();
                    self.start_save_file(token, client, runtime, model.id.clone());
                }
            }
            open = false;
        }
        if close_requested {
            open = false;
        }
        if !open {
            self.new_entry_kind = None;
            self.new_entry_name.clear();
        }
    }

    fn entry_action_windows(
        &mut self,
        ctx: &egui::Context,
        language: Language,
        token: &str,
        client: &ApiClient,
        runtime: &Runtime,
        model: &OrganismModel,
    ) {
        if self.selected_entry.is_some() && !self.rename_name.is_empty() {
            let mut open = true;
            let mut confirm = false;
            let mut close_requested = false;
            egui::Window::new(text(language, "creatures.rename"))
                .id(egui::Id::new("organism_model_rename_entry"))
                .open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(text(language, "creatures.rename_to"));
                    ui.add(egui::TextEdit::singleline(&mut self.rename_name).desired_width(280.0));
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !self.pending && !self.rename_name.trim().is_empty(),
                                egui::Button::new(text(language, "creatures.confirm")),
                            )
                            .clicked()
                        {
                            confirm = true;
                        }
                        if ui.button(text(language, "common.cancel")).clicked() {
                            close_requested = true;
                        }
                    });
                });
            if confirm {
                if let Some(path) = self.selected_entry.clone() {
                    let name = self.rename_name.trim().to_string();
                    self.start_rename_entry(token, client, runtime, model.id.clone(), path, name);
                }
                open = false;
            }
            if close_requested {
                open = false;
            }
            if !open {
                self.rename_name.clear();
            }
        }

        if let Some(path) = self.delete_entry_target.clone() {
            let mut open = true;
            let mut confirm = false;
            let mut close_requested = false;
            egui::Window::new(text(language, "creatures.delete_entry_confirm"))
                .id(egui::Id::new("organism_model_delete_entry"))
                .open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "{}: {path}",
                        text(language, "creatures.delete_entry_confirm")
                    ));
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !self.pending,
                                egui::Button::new(text(language, "creatures.confirm")),
                            )
                            .clicked()
                        {
                            confirm = true;
                        }
                        if ui.button(text(language, "common.cancel")).clicked() {
                            close_requested = true;
                        }
                    });
                });
            if confirm {
                self.start_delete_entry(token, client, runtime, model.id.clone(), path);
                open = false;
            }
            if close_requested {
                open = false;
            }
            if !open {
                self.delete_entry_target = None;
            }
        }
    }

    fn model_delete_window(
        &mut self,
        ctx: &egui::Context,
        language: Language,
        token: &str,
        client: &ApiClient,
        runtime: &Runtime,
        model: &OrganismModel,
    ) {
        if !self.delete_confirmed {
            return;
        }
        let mut open = true;
        let mut confirm = false;
        let mut close_requested = false;
        egui::Window::new(text(language, "creatures.delete_confirm"))
            .id(egui::Id::new("organism_model_delete"))
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(text(language, "creatures.delete_model_warning"));
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.pending,
                            egui::Button::new(text(language, "creatures.confirm")),
                        )
                        .clicked()
                    {
                        confirm = true;
                    }
                    if ui.button(text(language, "common.cancel")).clicked() {
                        close_requested = true;
                    }
                });
            });
        if confirm {
            self.start_delete(token, client, runtime, model.id.clone());
            open = false;
        }
        if close_requested {
            open = false;
        }
        if !open {
            self.delete_confirmed = false;
        }
    }

    fn start_load(&mut self, token: &str, client: &ApiClient, runtime: &Runtime) {
        if self.pending {
            return;
        }
        self.pending = true;
        let token = token.to_string();
        let client = client.clone();
        let sender = self.sender.clone();
        let generation = self.generation;
        runtime.spawn(async move {
            let result = async {
                let models = client.organism_models(&token).await?;
                let catalog = client.organism_components().await?;
                Ok((models, catalog))
            }
            .await;
            let _ = sender.send(RequestResult::Models { generation, result });
        });
    }

    fn clear_file_workspace(&mut self) {
        self.editor_path.clear();
        self.editor_content.clear();
        self.editor_open = false;
        self.new_folder_name.clear();
        self.new_entry_name.clear();
        self.new_entry_kind = None;
        self.delete_entry_target = None;
        self.selected_entry = None;
        self.rename_name.clear();
    }

    fn open_builder(&mut self, token: &str, client: &ApiClient, runtime: &Runtime) {
        self.building = true;
        self.catalog.clear();
        self.unlocked_ids.clear();
        self.clear_draft();
        self.start_catalog(token, client, runtime);
    }

    fn clear_draft(&mut self) {
        self.draft_name.clear();
        self.draft_components.clear();
        self.draft_connections.clear();
        self.selected_instance = None;
        self.connection_error = None;
        self.creation_error = None;
    }

    fn creation_issue(&self) -> Option<&'static str> {
        if self.pending {
            Some("creatures.create_pending")
        } else if self.draft_name.trim().is_empty() {
            Some("creatures.create_name_required")
        } else if self.draft_name.trim().chars().count() > 128 {
            Some("creatures.create_name_too_long")
        } else if self
            .draft_components
            .first()
            .is_none_or(|component| component.category != "body")
        {
            Some("creatures.create_body_required")
        } else if self.draft_components.len() > 256 {
            Some("creatures.create_too_many_components")
        } else if !self.is_draft_connected() {
            Some("creatures.graph_disconnected")
        } else {
            None
        }
    }

    fn can_add_component(&self, component: &OrganismComponent) -> bool {
        if !self
            .draft_components
            .iter()
            .any(|component| component.category == "body")
        {
            component.category == "body"
        } else {
            true
        }
    }

    fn is_draft_connected(&self) -> bool {
        let Some(body) = self
            .draft_components
            .first()
            .filter(|component| component.category == "body")
        else {
            return false;
        };

        let mut visited = BTreeSet::from([body.id.clone()]);
        let mut pending = vec![body.id.clone()];
        while let Some(current) = pending.pop() {
            for connection in &self.draft_connections {
                let next = if connection.first == current {
                    Some(&connection.second)
                } else if connection.second == current {
                    Some(&connection.first)
                } else {
                    None
                };
                if let Some(next) = next
                    && visited.insert(next.clone())
                {
                    pending.push(next.clone());
                }
            }
        }
        visited.len() == self.draft_components.len()
    }

    fn remove_draft_component(&mut self, id: &str) {
        self.draft_components.retain(|component| component.id != id);
        self.draft_connections
            .retain(|connection| connection.first != id && connection.second != id);
        if self.selected_instance.as_deref() == Some(id) {
            self.selected_instance = None;
        }
        if self
            .draft_components
            .first()
            .is_some_and(|component| component.category != "body")
            && let Some(body_index) = self
                .draft_components
                .iter()
                .position(|component| component.category == "body")
        {
            let body = self.draft_components.remove(body_index);
            self.draft_components.insert(0, body);
        }
        self.connection_error = None;
    }

    fn select_connection_endpoint(&mut self, id: String) {
        self.connection_error = None;
        let Some(first) = self.selected_instance.take() else {
            if self.is_connected_to_body(&id) {
                self.selected_instance = Some(id);
            } else {
                self.connection_error = Some("creatures.connection_source_disconnected");
            }
            return;
        };
        if first == id {
            return;
        }
        if !self.can_connect_components(&first, &id) {
            self.selected_instance = Some(first);
            self.connection_error = Some("creatures.connection_slots_unavailable");
            return;
        }
        let (first, second) = if first < id { (first, id) } else { (id, first) };
        self.draft_connections
            .push(CreateModelConnection { first, second });
    }

    fn is_connected_to_body(&self, id: &str) -> bool {
        let Some(body) = self
            .draft_components
            .first()
            .filter(|component| component.category == "body")
        else {
            return false;
        };
        let mut visited = BTreeSet::from([body.id.clone()]);
        let mut pending = vec![body.id.clone()];
        while let Some(current) = pending.pop() {
            for connection in &self.draft_connections {
                let next = if connection.first == current {
                    Some(&connection.second)
                } else if connection.second == current {
                    Some(&connection.first)
                } else {
                    None
                };
                if let Some(next) = next
                    && visited.insert(next.clone())
                {
                    pending.push(next.clone());
                }
            }
        }
        visited.contains(id)
    }

    fn can_connect_components(&self, first: &str, second: &str) -> bool {
        if first == second
            || self.draft_connections.iter().any(|connection| {
                (connection.first == first && connection.second == second)
                    || (connection.first == second && connection.second == first)
            })
        {
            return false;
        }
        let Some(first_component) = self
            .draft_components
            .iter()
            .find(|component| component.id == first)
        else {
            return false;
        };
        let Some(second_component) = self
            .draft_components
            .iter()
            .find(|component| component.id == second)
        else {
            return false;
        };
        self.available_slots_for(first_component, &second_component.category) > 0
            && self.available_slots_for(second_component, &first_component.category) > 0
    }

    fn available_slots_for(&self, component: &DraftComponent, target_category: &str) -> u32 {
        let capacity = component.slots.get(target_category).copied().unwrap_or(0);
        let used = self
            .draft_connections
            .iter()
            .filter_map(|connection| {
                if connection.first == component.id {
                    Some(connection.second.as_str())
                } else if connection.second == component.id {
                    Some(connection.first.as_str())
                } else {
                    None
                }
            })
            .filter_map(|other_id| {
                self.draft_components
                    .iter()
                    .find(|other| other.id == other_id)
            })
            .filter(|other| other.category == target_category)
            .count() as u32;
        capacity.saturating_sub(used)
    }

    fn start_catalog(&mut self, token: &str, client: &ApiClient, runtime: &Runtime) {
        if self.pending {
            return;
        }
        self.pending = true;
        let token = token.to_string();
        let client = client.clone();
        let sender = self.sender.clone();
        let generation = self.generation;
        runtime.spawn(async move {
            let result = async {
                let catalog = client.organism_components().await?;
                let unlocked = client.unlocked_organism_components(&token).await?;
                Ok((catalog, unlocked))
            }
            .await;
            let _ = sender.send(RequestResult::Catalog { generation, result });
        });
    }

    fn start_create(&mut self, token: &str, client: &ApiClient, runtime: &Runtime) {
        if self.pending {
            return;
        }
        self.pending = true;
        let token = token.to_string();
        let name = self.draft_name.trim().to_string();
        let components = self
            .draft_components
            .iter()
            .map(|component| CreateModelComponent {
                id: component.id.clone(),
                component_id: component.component_id.clone(),
            })
            .collect();
        let connections = self.draft_connections.clone();
        let client = client.clone();
        let sender = self.sender.clone();
        let generation = self.generation;
        runtime.spawn(async move {
            let result = client
                .create_organism_model(&token, &name, components, connections)
                .await;
            let _ = sender.send(RequestResult::Created { generation, result });
        });
    }

    fn start_folder(
        &mut self,
        token: &str,
        client: &ApiClient,
        runtime: &Runtime,
        model_id: String,
        path: String,
    ) {
        if self.pending {
            return;
        }
        self.pending = true;
        let token = token.to_string();
        let client = client.clone();
        let sender = self.sender.clone();
        let generation = self.generation;
        runtime.spawn(async move {
            let result = client.organism_model_folder(&token, &model_id, &path).await;
            let _ = sender.send(RequestResult::Folder {
                generation,
                model_id,
                path,
                result,
            });
        });
    }

    fn start_load_file(
        &mut self,
        token: &str,
        client: &ApiClient,
        runtime: &Runtime,
        model_id: String,
        path: String,
    ) {
        if self.pending {
            return;
        }
        self.pending = true;
        let token = token.to_string();
        let client = client.clone();
        let sender = self.sender.clone();
        let generation = self.generation;
        runtime.spawn(async move {
            let result = client
                .organism_model_file(&token, &model_id, &path)
                .await
                .and_then(|content| {
                    String::from_utf8(content)
                        .map_err(|error| ApiError::InvalidResponse(error.to_string()))
                });
            let _ = sender.send(RequestResult::FileLoaded {
                generation,
                model_id,
                path,
                result,
            });
        });
    }

    fn start_create_folder(
        &mut self,
        token: &str,
        client: &ApiClient,
        runtime: &Runtime,
        model_id: String,
        path: String,
    ) {
        if self.pending {
            return;
        }
        self.pending = true;
        let token = token.to_string();
        let client = client.clone();
        let sender = self.sender.clone();
        let generation = self.generation;
        let folder_path = self.folder_path.clone();
        runtime.spawn(async move {
            let result = async {
                client
                    .create_organism_model_folder(&token, &model_id, &path)
                    .await?;
                client
                    .organism_model_folder(&token, &model_id, &folder_path)
                    .await
            }
            .await;
            let _ = sender.send(RequestResult::FilesChanged {
                generation,
                model_id,
                folder_path,
                notice_key: "creatures.folder_created",
                result,
            });
        });
    }

    fn start_save_file(
        &mut self,
        token: &str,
        client: &ApiClient,
        runtime: &Runtime,
        model_id: String,
    ) {
        if self.pending {
            return;
        }
        self.pending = true;
        let token = token.to_string();
        let path = self.editor_path.trim().to_string();
        let content = self.editor_content.clone();
        let folder_path = self.folder_path.clone();
        let client = client.clone();
        let sender = self.sender.clone();
        let generation = self.generation;
        runtime.spawn(async move {
            let result = async {
                client
                    .write_organism_model_file(&token, &model_id, &path, content)
                    .await?;
                client
                    .organism_model_folder(&token, &model_id, &folder_path)
                    .await
            }
            .await;
            let _ = sender.send(RequestResult::FilesChanged {
                generation,
                model_id,
                folder_path,
                notice_key: "creatures.file_saved",
                result,
            });
        });
    }

    fn start_import_files(
        &mut self,
        token: &str,
        client: &ApiClient,
        runtime: &Runtime,
        model_id: String,
        folders: Vec<String>,
        files: Vec<(String, Vec<u8>)>,
    ) {
        if self.pending || (folders.is_empty() && files.is_empty()) {
            return;
        }
        self.pending = true;
        let token = token.to_string();
        let client = client.clone();
        let sender = self.sender.clone();
        let generation = self.generation;
        let folder_path = self.folder_path.clone();
        runtime.spawn(async move {
            let result = async {
                for path in folders {
                    client
                        .create_organism_model_folder(&token, &model_id, &path)
                        .await?;
                }
                for (path, content) in files {
                    client
                        .upload_organism_model_file(&token, &model_id, &path, content)
                        .await?;
                }
                client
                    .organism_model_folder(&token, &model_id, &folder_path)
                    .await
            }
            .await;
            let _ = sender.send(RequestResult::FilesChanged {
                generation,
                model_id,
                folder_path,
                notice_key: "creatures.file_saved",
                result,
            });
        });
    }

    fn start_delete_entry(
        &mut self,
        token: &str,
        client: &ApiClient,
        runtime: &Runtime,
        model_id: String,
        path: String,
    ) {
        if self.pending {
            return;
        }
        self.pending = true;
        let token = token.to_string();
        let folder_path = self.folder_path.clone();
        let client = client.clone();
        let sender = self.sender.clone();
        let generation = self.generation;
        runtime.spawn(async move {
            let result = async {
                client
                    .delete_organism_model_entry(&token, &model_id, &path)
                    .await?;
                client
                    .organism_model_folder(&token, &model_id, &folder_path)
                    .await
            }
            .await;
            let _ = sender.send(RequestResult::FilesChanged {
                generation,
                model_id,
                folder_path,
                notice_key: "creatures.entry_deleted",
                result,
            });
        });
    }

    fn start_rename_entry(
        &mut self,
        token: &str,
        client: &ApiClient,
        runtime: &Runtime,
        model_id: String,
        path: String,
        name: String,
    ) {
        if self.pending {
            return;
        }
        self.pending = true;
        let token = token.to_string();
        let folder_path = self.folder_path.clone();
        let client = client.clone();
        let sender = self.sender.clone();
        let generation = self.generation;
        runtime.spawn(async move {
            let result = async {
                client
                    .rename_organism_model_entry(&token, &model_id, &path, &name)
                    .await?;
                client
                    .organism_model_folder(&token, &model_id, &folder_path)
                    .await
            }
            .await;
            let _ = sender.send(RequestResult::FilesChanged {
                generation,
                model_id,
                folder_path,
                notice_key: "creatures.entry_renamed",
                result,
            });
        });
    }

    fn start_delete(
        &mut self,
        token: &str,
        client: &ApiClient,
        runtime: &Runtime,
        model_id: String,
    ) {
        if self.pending {
            return;
        }
        self.pending = true;
        let token = token.to_string();
        let client = client.clone();
        let sender = self.sender.clone();
        let generation = self.generation;
        runtime.spawn(async move {
            let _ = sender.send(RequestResult::Deleted {
                generation,
                result: client.delete_organism_model(&token, &model_id).await,
            });
        });
    }
}

fn structure_panel(
    ui: &mut egui::Ui,
    language: Language,
    model: &OrganismModel,
    catalog: &[OrganismComponent],
) {
    egui::Frame::default()
        .fill(theme::SURFACE)
        .stroke(egui::Stroke::new(1.0, theme::GRID))
        .corner_radius(8)
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.set_min_height(ui.available_height());
            ui.strong(text(language, "creatures.structure"));
            ui.horizontal(|ui| {
                ui.weak(format!(
                    "{} {}",
                    model.composition.components.len(),
                    text(language, "creatures.components")
                ));
                ui.separator();
                ui.weak(format!(
                    "{} {}",
                    model.composition.connections.len(),
                    text(language, "creatures.connections")
                ));
            });
            ui.separator();
            let canvas_size = egui::vec2(ui.available_width(), ui.available_height().max(260.0));
            let (canvas, _) = ui.allocate_exact_size(canvas_size, egui::Sense::hover());
            draw_model_graph(ui, canvas, model, catalog, language);
        });
}

fn draw_model_graph(
    ui: &egui::Ui,
    canvas: egui::Rect,
    model: &OrganismModel,
    catalog: &[OrganismComponent],
    language: Language,
) {
    let painter = ui.painter_at(canvas);
    painter.rect_filled(canvas, 6.0, theme::BACKGROUND);
    let node_size = egui::vec2(canvas.width().min(132.0).max(92.0), 62.0);
    let mut positions = BTreeMap::new();
    let mut group_counts = BTreeMap::<String, usize>::new();

    for component in model.composition.components.values() {
        let category = catalog
            .iter()
            .find(|definition| definition.id == component.component)
            .map(|definition| definition.category.as_str())
            .unwrap_or("unknown");
        let index = group_counts.entry(category.to_string()).or_default();
        let x = match category {
            "internal_organ" => 14.0,
            "external_organ" => (canvas.width() - node_size.x - 14.0).max(14.0),
            _ => ((canvas.width() - node_size.x) * 0.5).max(0.0),
        };
        let y = (18.0 + *index as f32 * 84.0).min((canvas.height() - node_size.y - 8.0).max(0.0));
        positions.insert(component.id.clone(), canvas.min + egui::vec2(x, y));
        *index += 1;
    }

    for connection in &model.composition.connections {
        if let (Some(first), Some(second)) = (
            positions.get(&connection.first),
            positions.get(&connection.second),
        ) {
            painter.line_segment(
                [*first + node_size * 0.5, *second + node_size * 0.5],
                egui::Stroke::new(2.5, theme::ACCENT),
            );
        }
    }

    for component in model.composition.components.values() {
        let Some(position) = positions.get(&component.id) else {
            continue;
        };
        let definition = catalog
            .iter()
            .find(|definition| definition.id == component.component);
        let category = definition
            .map(|definition| definition.category.as_str())
            .unwrap_or("unknown");
        let name = definition
            .map(|definition| definition.name.as_str())
            .unwrap_or(component.component.as_str());
        let rect = egui::Rect::from_min_size(*position, node_size);
        let color = slot_color(category);
        painter.rect_filled(rect, 6.0, theme::SURFACE);
        painter.rect_stroke(
            rect,
            6.0,
            egui::Stroke::new(1.5, color),
            egui::StrokeKind::Inside,
        );
        painter.text(
            egui::pos2(rect.center().x, rect.top() + 22.0),
            egui::Align2::CENTER_CENTER,
            truncate_label(name, 13),
            egui::FontId::proportional(14.0),
            egui::Color32::WHITE,
        );
        painter.text(
            egui::pos2(rect.center().x, rect.top() + 43.0),
            egui::Align2::CENTER_CENTER,
            category_label(language, category),
            egui::FontId::proportional(11.0),
            theme::MUTED_TEXT,
        );
    }
}

struct GraphActions {
    clicked: Option<String>,
    removed: Option<String>,
}

fn draw_draft_graph(
    ui: &mut egui::Ui,
    canvas: egui::Rect,
    components: &mut [DraftComponent],
    connections: &[CreateModelConnection],
    selected_id: Option<&str>,
    language: Language,
) -> GraphActions {
    const NODE_HEIGHT: f32 = 84.0;
    let node_width = canvas.width().min(144.0).max(96.0);
    let node_size = egui::vec2(node_width, NODE_HEIGHT);
    let painter = ui.painter_at(canvas);
    painter.rect_filled(canvas, 6.0, theme::BACKGROUND);

    let grid_color = theme::GRID.gamma_multiply(0.45);
    let mut x = canvas.left() + 24.0;
    while x < canvas.right() {
        painter.line_segment(
            [egui::pos2(x, canvas.top()), egui::pos2(x, canvas.bottom())],
            egui::Stroke::new(1.0, grid_color),
        );
        x += 32.0;
    }
    let mut y = canvas.top() + 24.0;
    while y < canvas.bottom() {
        painter.line_segment(
            [egui::pos2(canvas.left(), y), egui::pos2(canvas.right(), y)],
            egui::Stroke::new(1.0, grid_color),
        );
        y += 32.0;
    }

    let mut clicked = None;
    let mut removed = None;
    let mut hovered = BTreeSet::new();
    let mut remove_hovered = BTreeSet::new();
    for component in components.iter_mut() {
        component.position.x = component
            .position
            .x
            .clamp(0.0, (canvas.width() - node_size.x).max(0.0));
        component.position.y = component
            .position
            .y
            .clamp(0.0, (canvas.height() - node_size.y).max(0.0));
        let node_rect = egui::Rect::from_min_size(canvas.min + component.position, node_size);
        let response = ui
            .interact(
                node_rect,
                ui.make_persistent_id(("organism_graph_node", component.id.as_str())),
                egui::Sense::click_and_drag(),
            )
            .on_hover_cursor(egui::CursorIcon::Grab);
        if response.dragged() {
            component.position += response.drag_delta();
            component.position.x = component
                .position
                .x
                .clamp(0.0, (canvas.width() - node_size.x).max(0.0));
            component.position.y = component
                .position
                .y
                .clamp(0.0, (canvas.height() - node_size.y).max(0.0));
        }
        if response.clicked() {
            clicked = Some(component.id.clone());
        }
        if response.hovered() {
            hovered.insert(component.id.clone());
        }

        let final_rect = egui::Rect::from_min_size(canvas.min + component.position, node_size);
        let remove_rect = egui::Rect::from_center_size(
            egui::pos2(final_rect.right() - 11.0, final_rect.top() + 11.0),
            egui::vec2(20.0, 20.0),
        );
        let remove_response = ui
            .interact(
                remove_rect,
                ui.make_persistent_id(("organism_graph_remove", component.id.as_str())),
                egui::Sense::click(),
            )
            .on_hover_text(text(language, "creatures.remove_component"));
        if remove_response.clicked() {
            removed = Some(component.id.clone());
        }
        if remove_response.hovered() {
            remove_hovered.insert(component.id.clone());
        }
    }

    let center = |id: &str| component_center(components, id, canvas, node_size);
    for (connection_index, connection) in connections.iter().enumerate() {
        let first_component = components
            .iter()
            .find(|component| component.id == connection.first);
        let second_component = components
            .iter()
            .find(|component| component.id == connection.second);
        if let (Some(first_component), Some(second_component)) = (first_component, second_component)
        {
            let first_ordinal = connection_slot_ordinal(
                components,
                connections,
                connection_index,
                &first_component.id,
                &second_component.category,
            );
            let second_ordinal = connection_slot_ordinal(
                components,
                connections,
                connection_index,
                &second_component.id,
                &first_component.category,
            );
            let first = slot_position(
                first_component,
                &second_component.category,
                first_ordinal,
                canvas,
                node_size,
            );
            let second = slot_position(
                second_component,
                &first_component.category,
                second_ordinal,
                canvas,
                node_size,
            );
            painter.line_segment([first, second], egui::Stroke::new(3.0, theme::ACCENT));
            painter.circle_filled(first, 4.0, theme::ACCENT);
            painter.circle_filled(second, 4.0, theme::ACCENT);
        }
    }
    if let Some(selected) = selected_id
        && let Some(first) = center(selected)
        && let Some(pointer) = ui.ctx().pointer_hover_pos()
        && canvas.contains(pointer)
    {
        painter.line_segment(
            [first, pointer],
            egui::Stroke::new(1.5, theme::ACCENT.gamma_multiply(0.65)),
        );
    }

    if components.is_empty() {
        painter.text(
            canvas.center(),
            egui::Align2::CENTER_CENTER,
            text(language, "creatures.empty_structure"),
            egui::FontId::proportional(14.0),
            theme::MUTED_TEXT,
        );
    }
    for component in components.iter() {
        let rect = egui::Rect::from_min_size(canvas.min + component.position, node_size);
        let selected = selected_id == Some(component.id.as_str());
        let fill = if selected {
            theme::ACCENT.gamma_multiply(0.22)
        } else if hovered.contains(&component.id) {
            theme::SURFACE.gamma_multiply(1.25)
        } else {
            theme::SURFACE
        };
        let stroke = egui::Stroke::new(
            if selected { 2.0 } else { 1.0 },
            if selected { theme::ACCENT } else { theme::GRID },
        );
        painter.rect_filled(rect, 6.0, fill);
        painter.rect_stroke(rect, 6.0, stroke, egui::StrokeKind::Inside);
        painter.text(
            egui::pos2(rect.center().x, rect.top() + 20.0),
            egui::Align2::CENTER_CENTER,
            truncate_label(&component.name, 14),
            egui::FontId::proportional(14.0),
            egui::Color32::WHITE,
        );
        painter.text(
            egui::pos2(rect.center().x, rect.top() + 39.0),
            egui::Align2::CENTER_CENTER,
            category_label(language, &component.category),
            egui::FontId::proportional(11.0),
            theme::MUTED_TEXT,
        );
        let used = used_slot_counts(component, components, connections);
        let total_slots = component.slots.values().sum::<u32>();
        let used_slots = used.values().sum::<u32>();
        painter.text(
            egui::pos2(rect.center().x, rect.top() + 57.0),
            egui::Align2::CENTER_CENTER,
            format!(
                "{} {used_slots}/{total_slots}",
                text(language, "creatures.slots")
            ),
            egui::FontId::proportional(10.0),
            theme::MUTED_TEXT,
        );
        for (category, capacity) in &component.slots {
            let used_count = used.get(category).copied().unwrap_or(0);
            for ordinal in 0..*capacity {
                let port = slot_position(component, category, ordinal, canvas, node_size);
                painter.circle_filled(
                    port,
                    4.0,
                    if ordinal < used_count {
                        slot_color(category)
                    } else {
                        theme::BACKGROUND
                    },
                );
                painter.circle_stroke(port, 4.0, egui::Stroke::new(1.5, slot_color(category)));
            }
        }
        let remove_color = if remove_hovered.contains(&component.id) {
            theme::DANGER
        } else {
            theme::MUTED_TEXT
        };
        let remove_center = egui::pos2(rect.right() - 11.0, rect.top() + 11.0);
        painter.line_segment(
            [
                remove_center + egui::vec2(-3.5, -3.5),
                remove_center + egui::vec2(3.5, 3.5),
            ],
            egui::Stroke::new(1.5, remove_color),
        );
        painter.line_segment(
            [
                remove_center + egui::vec2(3.5, -3.5),
                remove_center + egui::vec2(-3.5, 3.5),
            ],
            egui::Stroke::new(1.5, remove_color),
        );
    }

    GraphActions { clicked, removed }
}

fn component_center(
    components: &[DraftComponent],
    id: &str,
    canvas: egui::Rect,
    node_size: egui::Vec2,
) -> Option<egui::Pos2> {
    components
        .iter()
        .find(|component| component.id == id)
        .map(|component| canvas.min + component.position + node_size * 0.5)
}

fn used_slot_counts(
    component: &DraftComponent,
    components: &[DraftComponent],
    connections: &[CreateModelConnection],
) -> std::collections::BTreeMap<String, u32> {
    let mut used = std::collections::BTreeMap::new();
    for connection in connections {
        let other_id = if connection.first == component.id {
            Some(connection.second.as_str())
        } else if connection.second == component.id {
            Some(connection.first.as_str())
        } else {
            None
        };
        if let Some(other) = other_id.and_then(|id| components.iter().find(|item| item.id == id)) {
            *used.entry(other.category.clone()).or_insert(0) += 1;
        }
    }
    used
}

fn connection_slot_ordinal(
    components: &[DraftComponent],
    connections: &[CreateModelConnection],
    connection_index: usize,
    component_id: &str,
    target_category: &str,
) -> u32 {
    connections[..connection_index]
        .iter()
        .filter_map(|connection| {
            if connection.first == component_id {
                Some(connection.second.as_str())
            } else if connection.second == component_id {
                Some(connection.first.as_str())
            } else {
                None
            }
        })
        .filter_map(|other_id| components.iter().find(|component| component.id == other_id))
        .filter(|component| component.category == target_category)
        .count() as u32
}

fn slot_position(
    component: &DraftComponent,
    target_category: &str,
    ordinal: u32,
    canvas: egui::Rect,
    node_size: egui::Vec2,
) -> egui::Pos2 {
    let total = component.slots.values().sum::<u32>().max(1);
    let preceding = component
        .slots
        .iter()
        .take_while(|(category, _)| category.as_str() != target_category)
        .map(|(_, count)| *count)
        .sum::<u32>();
    let slot_index = (preceding + ordinal).min(total - 1);
    let rect = egui::Rect::from_min_size(canvas.min + component.position, node_size);
    egui::pos2(
        rect.left() + (slot_index + 1) as f32 * rect.width() / (total + 1) as f32,
        rect.bottom(),
    )
}

fn slot_color(category: &str) -> egui::Color32 {
    match category {
        "body" => theme::WARNING,
        "internal_organ" => egui::Color32::from_rgb(98, 166, 235),
        "external_organ" => theme::ACCENT,
        _ => egui::Color32::from_rgb(190, 140, 220),
    }
}

fn next_node_position(index: usize) -> egui::Vec2 {
    if index == 0 {
        return egui::vec2(18.0, 18.0);
    }
    let column = (index - 1) % 2;
    let row = (index - 1) / 2;
    egui::vec2(18.0 + column as f32 * 164.0, 122.0 + row as f32 * 104.0)
}

fn truncate_label(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn short_id(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

fn category_label(language: Language, category: &str) -> String {
    match (language, category) {
        (Language::ZhCn, "external_organ") => "外部器官".to_string(),
        (Language::ZhCn, "body") => "身体".to_string(),
        (Language::ZhCn, "internal_organ") => "内部器官".to_string(),
        (_, "external_organ") => "external organ".to_string(),
        (_, "body") => "body".to_string(),
        (_, "internal_organ") => "internal organ".to_string(),
        (_, other) => other.replace('_', " "),
    }
}

fn slot_summary(language: Language, component: &OrganismComponent) -> String {
    if component.slots.counts.is_empty() {
        return format!("{}: 0", text(language, "creatures.slots"));
    }
    let slots = component
        .slots
        .counts
        .iter()
        .map(|(category, count)| format!("{} {count}", category_label(language, category)))
        .collect::<Vec<_>>()
        .join(" · ");
    format!("{}: {slots}", text(language, "creatures.slots"))
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn join_model_path(parent: &str, child: &str) -> String {
    let child = child.trim().trim_matches('/');
    if parent.is_empty() {
        child.to_string()
    } else {
        format!("{}/{}", parent.trim_end_matches('/'), child)
    }
}

fn display_folder_path(path: &str) -> String {
    if path.trim().is_empty() {
        "根目录".to_string()
    } else {
        path.trim_matches('/').replace('/', " / ")
    }
}

fn collect_import_entries(root: &Path, parent: &str) -> (Vec<String>, Vec<(String, Vec<u8>)>) {
    fn visit(
        root: &Path,
        current: &Path,
        parent: &str,
        folders: &mut Vec<String>,
        files: &mut Vec<(String, Vec<u8>)>,
    ) {
        let Ok(entries) = fs::read_dir(current) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let Ok(relative) = path.strip_prefix(root) else {
                    continue;
                };
                let relative = relative.to_string_lossy().replace('\\', "/");
                folders.push(join_model_path(parent, &relative));
                visit(root, &path, parent, folders, files);
            } else if path.is_file() {
                let Ok(content) = fs::read(&path) else {
                    continue;
                };
                let Ok(relative) = path.strip_prefix(root) else {
                    continue;
                };
                let relative = relative.to_string_lossy().replace('\\', "/");
                files.push((join_model_path(parent, &relative), content));
            }
        }
    }

    let mut folders = vec![parent.to_string()];
    let mut files = Vec::new();
    visit(root, root, parent, &mut folders, &mut files);
    (folders, files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(id: &str, category: &str) -> DraftComponent {
        let slots = match category {
            "body" => [
                ("external_organ".to_string(), 4),
                ("internal_organ".to_string(), 3),
            ]
            .into_iter()
            .collect(),
            "external_organ" | "internal_organ" => [("body".to_string(), 1)].into_iter().collect(),
            _ => Default::default(),
        };
        DraftComponent {
            id: id.to_string(),
            component_id: format!("{category}_component"),
            name: id.to_string(),
            category: category.to_string(),
            slots,
            position: egui::Vec2::ZERO,
        }
    }

    fn definition(id: &str, category: &str) -> OrganismComponent {
        OrganismComponent {
            id: id.to_string(),
            name: id.to_string(),
            category: category.to_string(),
            slots: Default::default(),
        }
    }

    #[test]
    fn body_must_be_first_but_more_bodies_can_be_added() {
        let mut page = CreaturesPage::new();
        let body = definition("body", "body");
        let sensor = definition("sensor", "external_organ");

        assert!(page.can_add_component(&body));
        assert!(!page.can_add_component(&sensor));

        page.draft_components.push(draft("body", "body"));
        assert!(page.can_add_component(&body));
        assert!(page.can_add_component(&sensor));
    }

    #[test]
    fn connections_must_start_from_a_body() {
        let mut page = CreaturesPage::new();
        page.draft_components = vec![draft("body", "body"), draft("sensor", "external_organ")];

        page.select_connection_endpoint("sensor".to_string());
        assert!(page.selected_instance.is_none());

        page.select_connection_endpoint("body".to_string());
        page.select_connection_endpoint("sensor".to_string());
        assert_eq!(page.draft_connections.len(), 1);
    }

    #[test]
    fn removing_a_component_removes_its_connections() {
        let mut page = CreaturesPage::new();
        page.draft_components = vec![draft("body", "body"), draft("sensor", "external_organ")];
        page.draft_connections.push(CreateModelConnection {
            first: "body".to_string(),
            second: "sensor".to_string(),
        });

        page.remove_draft_component("sensor");

        assert_eq!(page.draft_components.len(), 1);
        assert!(page.draft_connections.is_empty());
    }

    #[test]
    fn multiple_bodies_can_connect_through_a_compatible_component() {
        let mut page = CreaturesPage::new();
        let mut bridge = draft("bridge", "internal_organ");
        bridge.slots.insert("body".to_string(), 2);
        page.draft_components = vec![draft("body-1", "body"), bridge, draft("body-2", "body")];

        page.select_connection_endpoint("body-1".to_string());
        page.select_connection_endpoint("bridge".to_string());
        page.select_connection_endpoint("bridge".to_string());
        page.select_connection_endpoint("body-2".to_string());

        assert_eq!(page.draft_connections.len(), 2);
        assert!(page.is_draft_connected());
    }

    #[test]
    fn connections_stop_when_a_matching_slot_category_is_full() {
        let mut page = CreaturesPage::new();
        page.draft_components.push(draft("body", "body"));
        for index in 0..5 {
            page.draft_components
                .push(draft(&format!("sensor-{index}"), "external_organ"));
        }

        for index in 0..4 {
            page.select_connection_endpoint("body".to_string());
            page.select_connection_endpoint(format!("sensor-{index}"));
        }
        page.select_connection_endpoint("body".to_string());
        page.select_connection_endpoint("sensor-4".to_string());

        assert_eq!(page.draft_connections.len(), 4);
        assert_eq!(
            page.connection_error,
            Some("creatures.connection_slots_unavailable")
        );
    }

    #[test]
    fn every_component_must_be_connected_to_the_body() {
        let mut page = CreaturesPage::new();
        page.draft_components = vec![draft("body", "body"), draft("sensor", "external_organ")];
        assert!(!page.is_draft_connected());

        page.draft_connections.push(CreateModelConnection {
            first: "body".to_string(),
            second: "sensor".to_string(),
        });
        assert!(page.is_draft_connected());
    }

    #[test]
    fn create_validation_reports_the_specific_problem() {
        let mut page = CreaturesPage::new();
        assert_eq!(
            page.creation_issue(),
            Some("creatures.create_name_required")
        );

        page.draft_name = "test".to_string();
        assert_eq!(
            page.creation_issue(),
            Some("creatures.create_body_required")
        );

        page.draft_components = vec![draft("body", "body"), draft("sensor", "external_organ")];
        assert_eq!(page.creation_issue(), Some("creatures.graph_disconnected"));
    }

    #[test]
    fn joins_file_names_to_the_current_model_folder() {
        assert_eq!(join_model_path("", "main.rs"), "main.rs");
        assert_eq!(join_model_path("src", "main.rs"), "src/main.rs");
        assert_eq!(join_model_path("src/", "/main.rs"), "src/main.rs");
    }

    #[test]
    fn displays_model_paths_without_dot_segments() {
        assert_eq!(display_folder_path(""), "根目录");
        assert_eq!(display_folder_path("src/brain"), "src / brain");
    }
}
