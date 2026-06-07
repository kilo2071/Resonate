use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use std::cell::RefCell;

use crate::config::EffectEntry;
use crate::plugins::lv2;
use crate::plugins::{ParamKind, PluginParam};

// ── Inner state ───────────────────────────────────────────────────────────────

/// Runtime state for one chain entry (mirrors a config `EffectEntry`).
pub struct ChainEntry {
    id: String,
    display: String,
    enabled: bool,
}

type EnabledFn = Box<dyn Fn(usize, bool) + 'static>;
type ParamFn = Box<dyn Fn(usize, String, f32) + 'static>;
type AddFn = Box<dyn Fn(String) + 'static>;
type RemoveFn = Box<dyn Fn(usize) + 'static>;
type ParamProvider = Box<dyn Fn(usize) -> Vec<PluginParam> + 'static>;

// ── GObject subclass ──────────────────────────────────────────────────────────

mod imp {
    use super::*;

    #[derive(gtk::CompositeTemplate)]
    #[template(resource = "/io/github/kilo2071/Resonate/ui/effects_page.ui")]
    pub struct ResonateEffectsPage {
        #[template_child]
        pub add_effect_sheet: TemplateChild<adw::BottomSheet>,
        #[template_child]
        pub add_effect_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub effects_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub settings_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub effect_params_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub available_effects_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub effect_search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub input_source_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub chain_empty_label: TemplateChild<gtk::Label>,

        // Runtime state
        pub chain: RefCell<Vec<ChainEntry>>,
        pub selected_idx: RefCell<Option<usize>>,
        pub enabled_fn: RefCell<Option<EnabledFn>>,
        pub param_fn: RefCell<Option<ParamFn>>,
        pub add_fn: RefCell<Option<AddFn>>,
        pub remove_fn: RefCell<Option<RemoveFn>>,
        pub param_provider: RefCell<Option<ParamProvider>>,
        pub available_built: RefCell<bool>,
    }

    impl Default for ResonateEffectsPage {
        fn default() -> Self {
            Self {
                add_effect_sheet: Default::default(),
                add_effect_button: Default::default(),
                effects_list: Default::default(),
                settings_stack: Default::default(),
                effect_params_box: Default::default(),
                available_effects_list: Default::default(),
                effect_search_entry: Default::default(),
                input_source_label: Default::default(),
                chain_empty_label: Default::default(),
                chain: RefCell::new(Vec::new()),
                selected_idx: RefCell::new(None),
                enabled_fn: RefCell::new(None),
                param_fn: RefCell::new(None),
                add_fn: RefCell::new(None),
                remove_fn: RefCell::new(None),
                param_provider: RefCell::new(None),
                available_built: RefCell::new(false),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResonateEffectsPage {
        const NAME: &'static str = "ResonateEffectsPage";
        type Type = super::ResonateEffectsPage;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ResonateEffectsPage {
        fn constructed(&self) {
            self.parent_constructed();

            let sheet = self.add_effect_sheet.clone();
            self.add_effect_button.connect_clicked(move |_| {
                sheet.set_open(true);
            });

            self.obj().update_empty_label();
        }
    }

    impl WidgetImpl for ResonateEffectsPage {}
    impl BoxImpl for ResonateEffectsPage {}
}

glib::wrapper! {
    pub struct ResonateEffectsPage(ObjectSubclass<imp::ResonateEffectsPage>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl ResonateEffectsPage {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Populate the chain rows from config entries. Resets the params panel.
    pub fn init_chain(&self, entries: &[EffectEntry]) {
        let list = &self.imp().effects_list;
        while let Some(row) = list.first_child() {
            list.remove(&row);
        }
        self.imp().chain.borrow_mut().clear();
        *self.imp().selected_idx.borrow_mut() = None;
        self.imp().settings_stack.set_visible_child_name("empty");

        for (idx, entry) in entries.iter().enumerate() {
            self.append_chain_row(idx, entry);
        }

        // The add-sheet list is built lazily (LV2 discovery can be slow-ish).
        if !*self.imp().available_built.borrow() {
            self.populate_available_effects();
            *self.imp().available_built.borrow_mut() = true;
        }

        self.update_empty_label();
    }

    pub fn set_input_source_label(&self, name: &str) {
        self.imp().input_source_label.set_label(&format!("Input: {name}"));
    }

    pub fn connect_effect_enabled<F: Fn(usize, bool) + 'static>(&self, f: F) {
        *self.imp().enabled_fn.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_effect_param<F: Fn(usize, String, f32) + 'static>(&self, f: F) {
        *self.imp().param_fn.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_effect_add<F: Fn(String) + 'static>(&self, f: F) {
        *self.imp().add_fn.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_effect_remove<F: Fn(usize) + 'static>(&self, f: F) {
        *self.imp().remove_fn.borrow_mut() = Some(Box::new(f));
    }

    /// Supplies the live parameter list for the effect at `idx` (queried from the
    /// running chain), so the params panel works for built-ins and LV2 alike.
    pub fn connect_param_provider<F: Fn(usize) -> Vec<PluginParam> + 'static>(&self, f: F) {
        *self.imp().param_provider.borrow_mut() = Some(Box::new(f));
    }

    // ── Chain rows ──────────────────────────────────────────────────────────

    fn append_chain_row(&self, idx: usize, entry: &EffectEntry) {
        let display = display_name(&entry.id);

        let row = adw::ActionRow::builder()
            .title(&display)
            .activatable(true)
            .build();

        let toggle = gtk::Switch::builder()
            .active(entry.enabled)
            .valign(gtk::Align::Center)
            .build();
        toggle.connect_state_set(glib::clone!(
            #[weak(rename_to = page)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, state| {
                if let Some(f) = page.imp().enabled_fn.borrow().as_ref() {
                    f(idx, state);
                }
                if let Some(e) = page.imp().chain.borrow_mut().get_mut(idx) {
                    e.enabled = state;
                }
                glib::Propagation::Proceed
            }
        ));
        row.add_prefix(&toggle);

        let remove_btn = gtk::Button::builder()
            .icon_name("list-remove-symbolic")
            .tooltip_text("Remove effect")
            .valign(gtk::Align::Center)
            .css_classes(["flat", "circular"])
            .build();
        remove_btn.connect_clicked(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |_| {
                if let Some(f) = page.imp().remove_fn.borrow().as_ref() {
                    f(idx);
                }
            }
        ));
        row.add_suffix(&remove_btn);

        row.connect_activated(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |_| page.select_effect(idx)
        ));

        self.imp().effects_list.append(&row);
        self.imp().chain.borrow_mut().push(ChainEntry {
            id: entry.id.clone(),
            display,
            enabled: entry.enabled,
        });
    }

    fn select_effect(&self, idx: usize) {
        *self.imp().selected_idx.borrow_mut() = Some(idx);

        let params_box = &self.imp().effect_params_box;
        while let Some(child) = params_box.first_child() {
            params_box.remove(&child);
        }

        let display = self
            .imp()
            .chain
            .borrow()
            .get(idx)
            .map(|e| e.display.clone())
            .unwrap_or_default();

        let title = gtk::Label::builder()
            .label(&display)
            .xalign(0.0)
            .css_classes(["title-2"])
            .margin_bottom(8)
            .build();
        params_box.append(&title);

        let params = self
            .imp()
            .param_provider
            .borrow()
            .as_ref()
            .map(|f| f(idx))
            .unwrap_or_default();

        self.build_params(idx, &params, params_box);
        self.imp().settings_stack.set_visible_child_name("params");
    }

    /// Build a control per parameter, picking the widget from its `ParamKind`:
    /// switch (toggle), dropdown (enum), or slider (continuous/integer).
    fn build_params(&self, idx: usize, params: &[PluginParam], container: &gtk::Box) {
        if params.is_empty() {
            let label = gtk::Label::builder()
                .label("This effect has no adjustable parameters.")
                .xalign(0.0)
                .css_classes(["dim-label"])
                .build();
            container.append(&label);
            return;
        }

        let group = adw::PreferencesGroup::new();
        for p in params {
            let row = adw::ActionRow::builder().title(&p.label).build();
            match &p.kind {
                ParamKind::Toggle => self.build_toggle(idx, p, &row),
                ParamKind::Enum(points) => self.build_dropdown(idx, p, points, &row),
                ParamKind::Integer => self.build_slider(idx, p, &row, true),
                ParamKind::Continuous => self.build_slider(idx, p, &row, false),
            }
            group.add(&row);
        }
        container.append(&group);
    }

    fn build_slider(&self, idx: usize, p: &PluginParam, row: &adw::ActionRow, integer: bool) {
        let (lo, hi) = (p.min.min(p.max), p.min.max(p.max));
        let scale = gtk::Scale::builder()
            .orientation(gtk::Orientation::Horizontal)
            .draw_value(true)
            .value_pos(gtk::PositionType::Right)
            .width_request(220)
            .valign(gtk::Align::Center)
            .build();
        scale.set_range(lo as f64, hi as f64);
        if integer {
            scale.set_digits(0);
            scale.set_round_digits(0);
            scale.set_increments(1.0, 1.0);
        } else {
            scale.set_digits(if (hi - lo) >= 100.0 { 0 } else { 2 });
        }
        // Set value BEFORE connecting so initialisation doesn't fire the callback.
        scale.set_value(p.value.clamp(lo, hi) as f64);

        let param_id = p.id.clone();
        scale.connect_value_changed(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |s| {
                if let Some(f) = page.imp().param_fn.borrow().as_ref() {
                    f(idx, param_id.clone(), s.value() as f32);
                }
            }
        ));
        row.add_suffix(&scale);
    }

    fn build_toggle(&self, idx: usize, p: &PluginParam, row: &adw::ActionRow) {
        let sw = gtk::Switch::builder().valign(gtk::Align::Center).build();
        sw.set_active(p.value > 0.5);

        let param_id = p.id.clone();
        sw.connect_state_set(glib::clone!(
            #[weak(rename_to = page)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, state| {
                if let Some(f) = page.imp().param_fn.borrow().as_ref() {
                    f(idx, param_id.clone(), if state { 1.0 } else { 0.0 });
                }
                glib::Propagation::Proceed
            }
        ));
        row.add_suffix(&sw);
        row.set_activatable_widget(Some(&sw));
    }

    fn build_dropdown(
        &self,
        idx: usize,
        p: &PluginParam,
        points: &[(String, f32)],
        row: &adw::ActionRow,
    ) {
        let labels: Vec<&str> = points.iter().map(|(l, _)| l.as_str()).collect();
        let dropdown = gtk::DropDown::from_strings(&labels);
        dropdown.set_valign(gtk::Align::Center);

        // Pre-select the option whose value is closest to the current value.
        let selected = points
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                (a.1 - p.value)
                    .abs()
                    .partial_cmp(&(b.1 - p.value).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i as u32)
            .unwrap_or(0);
        dropdown.set_selected(selected);

        let param_id = p.id.clone();
        let values: Vec<f32> = points.iter().map(|(_, v)| *v).collect();
        dropdown.connect_selected_notify(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |d| {
                if let Some(v) = values.get(d.selected() as usize) {
                    if let Some(f) = page.imp().param_fn.borrow().as_ref() {
                        f(idx, param_id.clone(), *v);
                    }
                }
            }
        ));
        row.add_suffix(&dropdown);
    }

    // ── Available effects (add sheet) ─────────────────────────────────────────

    fn populate_available_effects(&self) {
        let list = &self.imp().available_effects_list;
        while let Some(row) = list.first_child() {
            list.remove(&row);
        }

        // Built-ins first, then installed LV2 plugins.
        let mut items: Vec<(String, String, String)> = vec![
            ("gate".into(), "Noise Gate".into(), "Silence audio below a threshold".into()),
            ("gain".into(), "Gain".into(), "Boost or cut the microphone volume".into()),
        ];
        for info in lv2::discover() {
            items.push((lv2::id_for_uri(&info.uri), info.name, "LV2 plugin".into()));
        }

        for (id, name, desc) in items {
            let row = adw::ActionRow::builder()
                .title(&name)
                .subtitle(&desc)
                .activatable(true)
                .build();
            let add_btn = gtk::Button::builder()
                .icon_name("list-add-symbolic")
                .tooltip_text("Add to chain")
                .valign(gtk::Align::Center)
                .css_classes(["flat", "circular"])
                .build();
            add_btn.connect_clicked(glib::clone!(
                #[weak(rename_to = page)]
                self,
                move |_| {
                    let already = page.imp().chain.borrow().iter().any(|e| e.id == id);
                    if !already {
                        if let Some(f) = page.imp().add_fn.borrow().as_ref() {
                            f(id.clone());
                        }
                    }
                    page.imp().add_effect_sheet.set_open(false);
                }
            ));
            row.add_suffix(&add_btn);
            list.append(&row);
        }

        let list_weak = list.downgrade();
        self.imp().effect_search_entry.connect_search_changed(move |entry| {
            let Some(list_clone) = list_weak.upgrade() else { return };
            let query = entry.text().to_lowercase();
            let mut i = 0;
            loop {
                let Some(row) = list_clone.row_at_index(i) else { break };
                // Rows are AdwActionRows appended directly, so each one *is* the
                // ListBoxRow — match against it, not its inner child.
                let visible = if query.is_empty() {
                    true
                } else {
                    row.downcast_ref::<adw::ActionRow>()
                        .map(|r| {
                            r.title().to_lowercase().contains(&query)
                                || r.subtitle().unwrap_or_default().to_lowercase().contains(&query)
                        })
                        .unwrap_or(false)
                };
                row.set_visible(visible);
                i += 1;
            }
        });
    }

    fn update_empty_label(&self) {
        let empty = self.imp().chain.borrow().is_empty();
        self.imp().chain_empty_label.set_visible(empty);
    }
}

/// Display name for a chain entry id: built-ins by name, LV2 by plugin name.
fn display_name(id: &str) -> String {
    match id {
        "gate" => "Noise Gate".to_string(),
        "gain" => "Gain".to_string(),
        other => lv2::uri_from_id(other)
            .and_then(lv2::name_for_uri)
            .unwrap_or_else(|| other.to_string()),
    }
}

impl Default for ResonateEffectsPage {
    fn default() -> Self {
        Self::new()
    }
}
