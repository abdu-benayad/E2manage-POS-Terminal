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

/// The start-up probe could not answer, and the till cannot say why.
///
/// # This names a limitation rather than a cause, on purpose
///
/// `AuthService::load_saved_session`, `OperatorSignIn::restore` and `PairingService`'s methods all
/// return `anyhow::Error`, which carries no discriminant. A store failure and an unreachable
/// platform arrive here indistinguishable, so this sentence claims neither. Picking one would be
/// inventing a cause, which is exactly what `UndeterminedCause` exists to stop.
///
/// Replace this the moment those services return typed errors: then the real cause is available
/// and `UndecidedNotice::for_cause` says something true and specific instead.
pub const STARTUP_PROBE_FAILED: Sentence = Sentence::new(
    "تعذّر على هذه النقطة إكمال فحوصات بدء التشغيل.",
    "This till could not complete its start-up checks.",
);

// ============================================================================
// Pairing, and the words a control needs
// ============================================================================

/// Offered wherever a retry could produce a different answer, and nowhere else.
pub const TRY_AGAIN: Sentence = Sentence::new("إعادة المحاولة", "Try again");

/// The destructive form of the pairing sentence.
///
/// The platform says this hardware is already enrolled, so approving this code **archives a
/// working terminal** — possibly one somebody is selling on right now. It is the only enrolment
/// state that warns, and it has to actually warn: an approval is not reversible from this screen.
pub const APPROVING_REPLACES_A_LIVE_TERMINAL: Sentence = Sentence::new(
    "تحذير: هذا الجهاز مسجّل بالفعل. الموافقة على هذا الرمز ستؤرشف نقطة البيع العاملة الحالية.",
    "Warning: this device is already enrolled. Approving this code will archive the working till it is registered as.",
);

/// The ordinary form. A first enrolment destroys nothing, so it says so plainly and does not
/// borrow the warning's tone.
pub const FIRST_ENROLMENT: Sentence = Sentence::new(
    "هذا تسجيل جديد لهذا الجهاز.",
    "This is a new enrolment for this device.",
);

/// Shown beside the code so somebody reading it aloud knows how long it is good for.
pub const CODE_EXPIRES_AT: Sentence = Sentence::new("ينتهي في", "Valid until");

/// The pairing screen's standing instruction.
pub const AWAITING_APPROVAL: Sentence = Sentence::new(
    "في انتظار الموافقة على هذا الرمز من لوحة التحكم.",
    "Waiting for this code to be approved in the back office.",
);

/// A pairing code could not be fetched at all.
pub const NO_PAIRING_CODE: Sentence = Sentence::new(
    "تعذّر الحصول على رمز اقتران.",
    "Could not get a pairing code.",
);

// ============================================================================
// Choosing an operator, and entering a PIN
// ============================================================================

/// The operator list's heading.
pub const CHOOSE_YOUR_NAME: Sentence = Sentence::new("اختر اسمك", "Choose your name");

/// No operators have ever synced to this till.
///
/// Deliberately not phrased as a fault. Sign-in works offline *once* operators have synced, so a
/// till with an empty roster is early rather than broken, and telling a shopkeeper otherwise sends
/// them looking for a problem that does not exist.
pub const NO_OPERATORS_YET: Sentence = Sentence::new(
    "لم تتم مزامنة أي مستخدمين بعد. صِل نقطة البيع بالشبكة مرة واحدة لجلب القائمة.",
    "No operators have synced yet. Connect this till once to fetch the list.",
);

/// The PIN screen's instruction.
pub const ENTER_YOUR_PIN: Sentence = Sentence::new("أدخل رمزك السري", "Enter your PIN");

/// The deliberate submit. The PIN keypad has no enter key of its own, so this is the only door.
pub const SIGN_IN: Sentence = Sentence::new("تسجيل الدخول", "Sign in");

/// Shown while a verification is in flight. There is no way out of this state and the words must
/// not imply one.
pub const CHECKING: Sentence = Sentence::new("جارٍ التحقق…", "Checking…");

/// Precedes the number of attempts left. Only ever rendered from an `AttemptsRemaining`, which no
/// outcome but a wrong PIN can produce.
pub const ATTEMPTS_REMAINING: Sentence = Sentence::new("محاولات متبقية", "attempts remaining");

// ============================================================================
// What an operator is
// ============================================================================
//
// The card under a name used to render `OperatorRole::to_string()`, which is `as_wire_str` —
// documented in `pos-models` as "the spelling the server and the store both use". That is a
// *protocol* token, and it reached the shop floor: an Arabic till showed `SUPERVISOR` in Latin
// capitals under an Arabic name. Naming the three here puts the roles where every other
// user-facing string in this screen already lives, and makes the wire spelling unreachable from
// the view by construction rather than by review.

/// Rings sales, and is granted nothing else by default.
pub const CASHIER: Sentence = Sentence::new("أمين صندوق", "Cashier");

/// Rings sales and authorises a cashier's voids, refunds and discounts.
pub const SUPERVISOR: Sentence = Sentence::new("مشرف", "Supervisor");

/// A supervisor who may also reach the till's settings.
pub const MANAGER: Sentence = Sentence::new("مدير", "Manager");

/// Every sentence in this module, for tests that must enumerate rather than reach.
///
/// **This is the witness for step 14's both-directions sweep.** A sentence absent from this array
/// is invisible to that sweep, so a test asserts the array's length against the number of
/// distinct sentences declared here.
pub const EVERY_SENTENCE: [Sentence; 30] = [
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
    STARTUP_PROBE_FAILED,
    TRY_AGAIN,
    APPROVING_REPLACES_A_LIVE_TERMINAL,
    FIRST_ENROLMENT,
    CODE_EXPIRES_AT,
    AWAITING_APPROVAL,
    NO_PAIRING_CODE,
    CHOOSE_YOUR_NAME,
    NO_OPERATORS_YET,
    ENTER_YOUR_PIN,
    SIGN_IN,
    CHECKING,
    ATTEMPTS_REMAINING,
    CASHIER,
    SUPERVISOR,
    MANAGER,
];
