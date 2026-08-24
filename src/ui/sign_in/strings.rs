//! Every sentence the sign-in screen can show, in one table.
//!
//! # Why the strings are here and not at the point of use
//!
//! Step 14 has to prove the screen renders correctly in both reading directions. A test can only
//! do that if it can *enumerate* the text — a sentence built at the call site, or assembled from
//! fragments, is reachable only by driving the screen into the state that produces it. Keeping
//! them in one table makes "every sentence, in both directions" a thing a test can iterate.
//!
//! # Why a [`Sentence`] carries both languages rather than a lookup by locale
//!
//! A locale-keyed lookup returns a `&str` and loses which language it is. That is fine until a
//! test wants to assert that the Arabic and the English of the *same* sentence differ, or that
//! neither is empty — at which point the test needs both halves at once and a lookup cannot give
//! them. Carrying the pair also makes a missing translation a compile error rather than a runtime
//! fallback to the wrong language, which is the failure this shape exists to prevent.
//!
//! Arabic is the default locale for this product, so it is the first field and the first argument.

/// One sentence, in both of the till's reading directions.
///
/// Not a `String`: nothing here is composed at runtime. A count or a required length is rendered
/// by an element that takes it as an argument — see [`PadOffer`](super::PadOffer) — rather than
/// interpolated into a message, so that no sentence can carry a number the caller did not
/// deliberately supply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sentence {
    arabic: &'static str,
    english: &'static str,
}

impl Sentence {
    /// Builds a sentence. Arabic first, because Arabic is this product's default locale and an
    /// argument order that matches the default is one fewer thing to get backwards.
    pub const fn new(arabic: &'static str, english: &'static str) -> Self {
        Self { arabic, english }
    }

    /// The Arabic text.
    pub const fn arabic(self) -> &'static str {
        self.arabic
    }

    /// The English text.
    pub const fn english(self) -> &'static str {
        self.english
    }
}

// ============================================================================
// Refusals — the platform or the till said no
// ============================================================================

/// The PIN did not match.
///
/// Deliberately says nothing about how many attempts remain. That count lives on
/// [`PadOffer::AtCost`](super::PadOffer::AtCost) and is rendered by an element that takes it as an
/// argument, so it cannot appear beside a message that has no count to show.
pub const WRONG_PIN: Sentence = Sentence::new("الرمز غير صحيح.", "That PIN is not correct.");

/// The account is locked.
pub const LOCKED: Sentence = Sentence::new(
    "تم قفل هذا الحساب. يلزم تأكيد هوية المستخدم لفتحه.",
    "This account is locked. Someone must confirm the operator's identity to unlock it.",
);

/// No such operator at this till.
pub const OPERATOR_UNKNOWN: Sentence = Sentence::new(
    "لا يوجد مستخدم بهذا المعرّف على هذه النقطة.",
    "No operator with that identifier is known to this till.",
);

/// The operator exists but is not active in HR.
pub const OPERATOR_INACTIVE: Sentence =
    Sentence::new("هذا المستخدم غير نشط.", "This operator is not active.");

/// The stored credential could not be read — the till's fault, not the operator's.
pub const CREDENTIAL_UNREADABLE: Sentence = Sentence::new(
    "تعذّرت قراءة بيانات الاعتماد المخزّنة على هذه النقطة.",
    "The credential stored on this till could not be read.",
);

/// The stored credential is past its expiry.
pub const CREDENTIAL_EXPIRED: Sentence = Sentence::new(
    "انتهت صلاحية بيانات الاعتماد المخزّنة، ويلزم الاتصال بالخادم.",
    "The stored credential has expired; the platform must be reached.",
);

/// The credential was enrolled under a length the tenant no longer requires.
///
/// Names no digit count: the required length rides on
/// [`PadOffer::FreeOfCharge`](super::PadOffer::FreeOfCharge), for the reason [`WRONG_PIN`] carries
/// no attempt count.
pub const CREDENTIAL_REQUIRES_ROTATION: Sentence = Sentence::new(
    "يجب تحديث رمزك ليطابق سياسة المنشأة.",
    "Your PIN must be updated to match the company's policy.",
);

// ============================================================================
// Undecided — the till could not find out
// ============================================================================

/// The platform could not be reached.
pub const SERVER_UNREACHABLE: Sentence = Sentence::new(
    "تعذّر الوصول إلى الخادم، ولم يتم التحقق من الرمز.",
    "The platform could not be reached, so the PIN was not checked.",
);

/// The local store could not answer.
pub const STORE_UNAVAILABLE: Sentence = Sentence::new(
    "تعذّر على التخزين المحلي في هذه النقطة الإجابة.",
    "This till's local store could not answer.",
);

/// The terminal session was rejected and could not be renewed.
pub const REAUTH_FAILED: Sentence = Sentence::new(
    "انتهت جلسة هذه النقطة وتعذّر تجديدها، ويلزم تسجيل دخولها من جديد.",
    "This till's session was rejected and could not be renewed; it must sign in again.",
);

/// The device was taken away. No remedy at the till.
///
/// Paired with [`ENROLMENT_SUSPENDED`], and the pair must never collapse into one sentence:
/// `Repudiation::has_a_remedy_at_the_till` is false only for this one, and telling someone their
/// device is gone when an administrator could restore it in thirty seconds sends them home for the
/// day.
pub const ENROLMENT_WITHDRAWN: Sentence = Sentence::new(
    "تم سحب هذا الجهاز من الأسطول.",
    "This device has been withdrawn from the fleet.",
);

/// The device is enrolled and not active — recoverable, and it names who can recover it.
pub const ENROLMENT_SUSPENDED: Sentence = Sentence::new(
    "هذا الجهاز موقوف، ويمكن لمسؤول إعادة تفعيله.",
    "This device is not active; an administrator can reactivate it.",
);

/// Half-provisioned: no `secretHash` on the platform, so pairing must happen again.
pub const TERMINAL_NOT_PROVISIONED: Sentence = Sentence::new(
    "هذا الجهاز غير مكتمل الإعداد، ويجب إقرانه من جديد.",
    "This device is not fully provisioned; it must be paired again.",
);

/// The platform answered and the till could not read the answer.
///
/// Says "report this" rather than "try again" on purpose: a contract breach means the two systems
/// disagree about the shape of an endpoint, and a screen that invites a retry turns somebody's bug
/// into weather the cashier is asked to wait out.
pub const CONTRACT_BREACH: Sentence = Sentence::new(
    "أجاب الخادم بصيغة تعذّرت قراءتها. أبلغ الدعم الفني.",
    "The platform answered in a form this till could not read. Report this to support.",
);

/// Every sentence in this module, for tests that must enumerate rather than reach.
///
/// **This is the witness for step 14's both-directions sweep.** A sentence absent from this array
/// is invisible to that sweep, so a test asserts the array's length against the number of
/// distinct sentences declared here.
pub const EVERY_SENTENCE: [Sentence; 14] = [
    WRONG_PIN,
    LOCKED,
    OPERATOR_UNKNOWN,
    OPERATOR_INACTIVE,
    CREDENTIAL_UNREADABLE,
    CREDENTIAL_EXPIRED,
    CREDENTIAL_REQUIRES_ROTATION,
    SERVER_UNREACHABLE,
    STORE_UNAVAILABLE,
    REAUTH_FAILED,
    ENROLMENT_WITHDRAWN,
    ENROLMENT_SUSPENDED,
    TERMINAL_NOT_PROVISIONED,
    CONTRACT_BREACH,
];
