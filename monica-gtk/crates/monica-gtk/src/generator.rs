use std::cell::RefCell;
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use monica_vault::{analyze_password, generate_password, GeneratorOptions};
use secrecy::ExposeSecret;

use crate::security::copy_secret_with_timeout;
use crate::state::AppState;
use crate::widgets::{field, value_label};

#[derive(Clone)]
pub struct GeneratorPage {
    pub root: gtk::Widget,
    output: gtk::Label,
    strength: gtk::Label,
    length: libadwaita::SpinRow,
    upper: libadwaita::SwitchRow,
    lower: libadwaita::SwitchRow,
    digits: libadwaita::SwitchRow,
    symbols: libadwaita::SwitchRow,
    current: Rc<RefCell<Option<secrecy::SecretString>>>,
}

impl GeneratorPage {
    pub fn build(state: &AppState) -> Self {
        let length = libadwaita::SpinRow::builder()
            .title("长度")
            .adjustment(&gtk::Adjustment::new(20.0, 8.0, 128.0, 1.0, 8.0, 0.0))
            .digits(0)
            .build();
        let upper = libadwaita::SwitchRow::builder()
            .title("大写")
            .active(true)
            .build();
        let lower = libadwaita::SwitchRow::builder()
            .title("小写")
            .active(true)
            .build();
        let digits = libadwaita::SwitchRow::builder()
            .title("数字")
            .active(true)
            .build();
        let symbols = libadwaita::SwitchRow::builder()
            .title("符号")
            .active(true)
            .build();

        let group = libadwaita::PreferencesGroup::builder()
            .title("字符")
            .build();
        group.add(&length);
        group.add(&upper);
        group.add(&lower);
        group.add(&digits);
        group.add(&symbols);

        let output = value_label("点「生成」");
        output.add_css_class("title-2");
        output.add_css_class("monospace");
        output.set_selectable(false);
        let strength = gtk::Label::builder()
            .label("")
            .xalign(0.0)
            .css_classes(["caption", "dim-label"])
            .build();

        let generate = gtk::Button::builder()
            .label("生成")
            .css_classes(["suggested-action", "pill"])
            .build();
        let copy = gtk::Button::builder()
            .label("复制")
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
                .label("生成器")
                .css_classes(["title-1"])
                .xalign(0.0)
                .build(),
        );
        form.append(&group);
        form.append(&field("结果", &output));
        form.append(&strength);
        form.append(&actions);
        form.append(
            &gtk::Label::builder()
                .label("编辑登录项时可点「生成」填入密码。")
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
                    state.show_error(None, "请先生成");
                    return;
                };
                copy_secret_with_timeout(
                    &copy,
                    &secret,
                    &state.toast,
                    &state.clipboard_generation,
                );
            }
        ));
        page
    }

    pub fn on_session_changed(&self, _state: &AppState) {}

    pub fn clear_sensitive(&self) {
        self.current.borrow_mut().take();
        self.output.set_label("点「生成」");
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
        self.strength
            .set_label(&format!("强度：{}", strength.label));
        *self.current.borrow_mut() = Some(secret);
    }
}

pub fn generate_default() -> secrecy::SecretString {
    generate_password(GeneratorOptions::default())
}
