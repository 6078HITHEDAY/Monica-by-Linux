use crate::generator::GeneratorPage;
use crate::lifecycle::{LifecycleKind, LifecyclePage};
use crate::notes::NotePage;
use crate::otp::OtpPage;
use crate::passwords::PasswordPage;
use crate::state::AppState;
use crate::timeline::TimelinePage;
use crate::wallet::WalletPage;

#[derive(Clone)]
pub struct Pages {
    pub passwords: PasswordPage,
    pub generator: GeneratorPage,
    pub otp: OtpPage,
    pub notes: NotePage,
    pub wallet: WalletPage,
    pub timeline: TimelinePage,
    pub recycle: LifecyclePage,
    pub archive: LifecyclePage,
}

impl Pages {
    pub fn build(state: &AppState) -> Self {
        Self {
            passwords: PasswordPage::build(state),
            generator: GeneratorPage::build(state),
            otp: OtpPage::build(state),
            notes: NotePage::build(state),
            wallet: WalletPage::build(state),
            timeline: TimelinePage::build(state),
            recycle: LifecyclePage::build(state, LifecycleKind::Trash),
            archive: LifecyclePage::build(state, LifecycleKind::Archive),
        }
    }

    pub fn on_session_changed(&self, state: &AppState) {
        self.passwords.on_session_changed(state);
        self.generator.on_session_changed(state);
        self.otp.on_session_changed(state);
        self.notes.on_session_changed(state);
        self.wallet.on_session_changed(state);
        self.timeline.on_session_changed(state);
        self.recycle.on_session_changed(state);
        self.archive.on_session_changed(state);
    }

    pub fn clear_sensitive(&self) {
        self.passwords.clear_sensitive();
        self.generator.clear_sensitive();
        self.otp.clear_sensitive();
        self.notes.clear_sensitive();
        self.wallet.clear_sensitive();
        self.timeline.clear_sensitive();
        self.recycle.clear_sensitive();
        self.archive.clear_sensitive();
    }
}
