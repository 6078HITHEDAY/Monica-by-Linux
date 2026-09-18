use std::cell::RefCell;
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use monica_vault::{analyze_password, generate_password, GeneratorOptions};
use secrecy::ExposeSecret;

use crate::i18n::{t, tf};
use crate::security::copy_secret_with_timeout;
use crate::state::AppState;
use crate::widgets::{field, value_label};

#[derive(Clone)]
pub struct GeneratorPage {
    pub root: gtk::Widget,
    output: gtk::Label,
    strength: gtk::Label,
    length: gtk::SpinButton,
    upper: libadwaita::SwitchRow,
    lower: libadwaita::SwitchRow,
    digits: libadwaita::SwitchRow,
    symbols: libadwaita::SwitchRow,
    current: Rc<RefCell<Option<secrecy::SecretString>>>,
}

impl GeneratorPage {
    pub fn build(state: &AppState) -> Self {
        let adjustment = gtk::Adjustment::new(20.0, 8.0, 128.0, 1.0, 8.0, 0.0);
        let scale = gtk::Scale::builder()
            .orientation(gtk::Orientation::Horizontal)
            .adjustment(&adjustment)
            .digits(0)
            .draw_value(false)
            .hexpand(true)
            .valign(gtk::Align::Center)
            .width_request(160)
            .build();
        scale.set_increments(1.0, 8.0);
        let length = gtk::SpinButton::builder()
            .adjustment(&adjustment)
            .digits(0)
            .numeric(true)
            .valign(gtk::Align::Center)
            .build();
        length.set_width_chars(3);
        let length_row = libadwaita::ActionRow::builder()
            .title(t("generator.length"))
            .activatable(false)
            .build();
        length_row.add_suffix(&scale);
        length_row.add_suffix(&length);
        let upper = libadwaita::SwitchRow::builder()
            .title(t("generator.upper"))
            .active(true)
            .build();
        let lower = libadwaita::SwitchRow::builder()
            .title(t("generator.lower"))
            .active(true)
            .build();
        let digits = libadwaita::SwitchRow::builder()
            .title(t("generator.digits"))
            .active(true)
            .build();
        let symbols = libadwaita::SwitchRow::builder()
            .title(t("generator.symbols"))
            .active(true)
            .build();

        let group = libadwaita::PreferencesGroup::builder()
            .title(t("generator.charset"))
            .build();
        group.add(&length_row);
        group.add(&upper);
        group.add(&lower);
        group.add(&digits);
        group.add(&symbols);

        let output = value_label(&t("generator.click"));
        output.add_css_class("title-2");
        output.add_css_class("monospace");
        output.set_selectable(false);
        let strength = gtk::Label::builder()
            .label("")
            .xalign(0.0)
            .css_classes(["caption", "dim-label"])
            .build();

        let generate = gtk::Button::builder()
            .label(t("generator.generate"))
            .css_classes(["suggested-action", "pill"])
            .build();
        let copy = gtk::Button::builder()
            .label(t("common.copy"))
            .css_classes(["pill"])
            .build();
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.append(&generate);
        actions.append(&copy);

        let form = gtk::Box::new(gtk::Orientation::Vertical, 16);
        form.set_valign(gtk::Align::Start);
        form.set_margin_start(18);
        form.set_margin_end(18);
        form.set_margin_top(18);
        form.set_margin_bottom(18);
        form.append(
            &gtk::Label::builder()
                .label(t("nav.generator"))
                .css_classes(["title-1"])
                .xalign(0.0)
                .build(),
        );
        form.append(&group);
        form.append(&field(&t("generator.result"), &output));
        form.append(&strength);
        form.append(&actions);
        form.append(
            &gtk::Label::builder()
                .label(t("generator.hint"))
                .wrap(true)
                .xalign(0.0)
                .css_classes(["caption", "dim-label"])
                .build(),
        );

        let page = Self {
            root: gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Never)
                .child(
                    &libadwaita::Clamp::builder()
                        .maximum_size(520)
                        .child(&form)
                        .build(),
                )
                .build()
                .upcast(),
            output,
            strength,
            length,
            upper,
            lower,
            digits,
            symbols,
            current: Rc::new(RefCell::new(None)),
        };

        generate.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            move |_| {
                state.touch();
                page.regenerate();
            }
        ));
        copy.connect_clicked(glib::clone!(
            #[strong]
            state,
            #[strong(rename_to = page)]
            page,
            #[weak]
            copy,
            move |_| {
                state.touch();
                let Some(secret) = page.current.borrow().clone() else {
                    state.show_error(None, &t("generator.need"));
                    return;
                };
                copy_secret_with_timeout(
                    &copy,
                    &secret,
                    &state,
                );
            }
        ));
        page
    }

    pub fn on_session_changed(&self, _state: &AppState) {}

    pub fn clear_sensitive(&self) {
        self.current.borrow_mut().take();
        self.output.set_label(&t("generator.click"));
        self.strength.set_label("");
    }

    fn options(&self) -> GeneratorOptions {
        GeneratorOptions {
            length: self.length.value() as usize,
            uppercase: self.upper.is_active(),
            lowercase: self.lower.is_active(),
            digits: self.digits.is_active(),
            symbols: self.symbols.is_active(),
        }
    }

    fn regenerate(&self) {
        let secret = generate_password(self.options());
        let strength = analyze_password(secret.expose_secret());
        self.output.set_label(secret.expose_secret());
        let label = strength_label(strength.score);
        self.strength
            .set_label(&tf("generator.strength", &[&label]));
        *self.current.borrow_mut() = Some(secret);
    }
}

fn strength_label(score: u8) -> String {
    match score {
        5 => t("generator.strength.very_strong"),
        4 => t("generator.strength.strong"),
        3 => t("generator.strength.medium"),
        2 => t("generator.strength.weak"),
        _ => t("generator.strength.very_weak"),
    }
}

pub fn generate_default() -> secrecy::SecretString {
    generate_password(GeneratorOptions::default())
}
