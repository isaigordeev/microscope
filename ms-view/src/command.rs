//! Vim grammar state machine.
//!
//! `VimMachine` parses key sequences according to vim's
//! `[count] operator [count] motion` grammar and emits
//! `Action` values. No editor dependency — pure state
//! machine.

// ── Key input (terminal-independent) ──────────────

/// Terminal-independent key representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyInput {
    pub code: KeyCode,
    pub ctrl: bool,
}

/// Key code variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    Char(char),
    Esc,
    Enter,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
}

// ── Operator ──────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Delete,
    Change,
    Yank,
    Indent,
    Dedent,
    Lowercase,
    Uppercase,
    ToggleCase,
}

// ── Motion ────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Down,
    Up,
    WordStart,
    WordStartBig,
    WordEnd,
    WordEndBig,
    BackWord,
    BackWordBig,
    LineStart,
    LineEnd,
    FirstNonBlank,
    GotoTop,
    GotoBottom,
    GotoLine,
    ParagraphForward,
    ParagraphBackward,
    FindChar(char),
    FindCharBack(char),
    TillChar(char),
    TillCharBack(char),
    MatchBracket,
    ScreenTop,
    ScreenMiddle,
    ScreenBottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionType {
    Charwise,
    Linewise,
}

/// Classify a motion as charwise or linewise.
impl Motion {
    #[must_use]
    pub const fn motion_type(self) -> MotionType {
        match self {
            Self::Down
            | Self::Up
            | Self::GotoTop
            | Self::GotoBottom
            | Self::GotoLine
            | Self::ParagraphForward
            | Self::ParagraphBackward
            | Self::ScreenTop
            | Self::ScreenMiddle
            | Self::ScreenBottom => MotionType::Linewise,
            _ => MotionType::Charwise,
        }
    }

    /// Whether this motion is inclusive (endpoint
    /// included in operator range). Exclusive motions
    /// exclude the endpoint.
    #[must_use]
    pub const fn is_inclusive(self) -> bool {
        match self {
            // Inclusive: endpoint IS part of the range
            Self::Right
            | Self::WordEnd
            | Self::WordEndBig
            | Self::LineEnd
            | Self::FindChar(_)
            | Self::FindCharBack(_)
            | Self::MatchBracket
            | Self::GotoTop
            | Self::GotoBottom
            | Self::GotoLine
            | Self::ScreenTop
            | Self::ScreenMiddle
            | Self::ScreenBottom => true,
            // Exclusive: endpoint is NOT in the range
            _ => false,
        }
    }
}

// ── Insert variant ────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertVariant {
    Before,
    After,
    LineEnd,
    LineStart,
    LineBelow,
    LineAbove,
}

// ── Special command ───────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialCommand {
    DeleteChar,
    DeleteCharBefore,
    Substitute,
    SubstituteLine,
    ReplaceChar(char),
    JoinLines,
    ToggleCaseChar,
    ChangeToEnd,
    DeleteToEnd,
    YankLine,
    IndentLine,
    DedentLine,
    Paste,
    PasteBefore,
    Undo,
    Redo,
    DotRepeat,
}

// ── Action (output) ───────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Pure cursor movement.
    Move(Motion, usize),
    /// Operator applied to a motion range.
    OperatorMotion { operator: Operator, motion: Motion, count: usize },
    /// Operator applied to whole lines (dd/yy/cc).
    OperatorLine { operator: Operator, count: usize },
    /// Enter insert mode.
    EnterInsert(InsertVariant),
    /// Enter command mode.
    EnterCommand,
    /// Special single-key command.
    Special(SpecialCommand, usize),
    /// No action (still pending or unrecognized).
    None,
}

// ── Internal state ────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
enum VimState {
    Normal,
    Count,
    OperatorPending {
        operator: Operator,
    },
    OperatorCount {
        operator: Operator,
    },
    WaitingChar {
        purpose: CharPurpose,
    },
    WaitingG,
    WaitingGOp {
        operator: Operator,
    },
    WaitingAngle {
        is_indent: bool,
    },
    #[allow(dead_code)]
    WaitingAngleOp {
        operator: Operator,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharPurpose {
    FindChar,
    FindCharBack,
    TillChar,
    TillCharBack,
    ReplaceChar,
    OpFindChar { operator: Operator },
    OpFindCharBack { operator: Operator },
    OpTillChar { operator: Operator },
    OpTillCharBack { operator: Operator },
}

// ── VimMachine ────────────────────────────────────

/// Vim grammar state machine.
#[derive(Debug)]
pub struct VimMachine {
    state: VimState,
    count1: Option<usize>,
    count2: Option<usize>,
}

impl VimMachine {
    #[must_use]
    pub const fn new() -> Self {
        Self { state: VimState::Normal, count1: None, count2: None }
    }

    /// Feed a key. Returns an Action when the sequence
    /// is complete, or `Action::None` if more keys are
    /// needed.
    pub fn feed(&mut self, key: KeyInput) -> Action {
        // Esc always resets
        if key.code == KeyCode::Esc {
            self.reset();
            return Action::None;
        }

        match self.state.clone() {
            VimState::Normal | VimState::Count => self.feed_normal(key),
            VimState::OperatorPending { operator } => {
                self.feed_operator_pending(key, operator)
            }
            VimState::OperatorCount { operator } => {
                self.feed_operator_count(key, operator)
            }
            VimState::WaitingChar { purpose } => {
                self.feed_waiting_char(key, purpose)
            }
            VimState::WaitingG => self.feed_waiting_g(key),
            VimState::WaitingGOp { operator } => {
                self.feed_waiting_g_op(key, operator)
            }
            VimState::WaitingAngle { is_indent } => {
                self.feed_waiting_angle(key, is_indent)
            }
            VimState::WaitingAngleOp { operator } => {
                self.feed_waiting_angle_op(key, operator)
            }
        }
    }

    /// Reset to Normal state.
    pub const fn reset(&mut self) {
        self.state = VimState::Normal;
        self.count1 = None;
        self.count2 = None;
    }

    /// Whether we're in operator-pending mode.
    #[must_use]
    pub const fn is_operator_pending(&self) -> bool {
        matches!(
            self.state,
            VimState::OperatorPending { .. }
                | VimState::OperatorCount { .. }
                | VimState::WaitingGOp { .. }
                | VimState::WaitingAngleOp { .. }
        )
    }

    fn effective_count(&self) -> usize {
        let c1 = self.count1.unwrap_or(1);
        let c2 = self.count2.unwrap_or(1);
        c1.saturating_mul(c2)
    }

    fn count1_val(&self) -> usize {
        self.count1.unwrap_or(1)
    }

    const fn emit_and_reset(&mut self, action: Action) -> Action {
        self.reset();
        action
    }

    // ── Normal/Count state ────────────────────────

    #[allow(clippy::too_many_lines)]
    fn feed_normal(&mut self, key: KeyInput) -> Action {
        // Ctrl combos
        if key.ctrl {
            return if key.code == KeyCode::Char('r') {
                self.emit_and_reset(Action::Special(
                    SpecialCommand::Redo,
                    self.count1_val(),
                ))
            } else {
                self.reset();
                Action::None
            };
        }

        match key.code {
            // Count accumulation
            KeyCode::Char(c @ '1'..='9') => {
                let d = c as usize - '0' as usize;
                self.count1 = Some(self.count1.unwrap_or(0) * 10 + d);
                self.state = VimState::Count;
                Action::None
            }
            KeyCode::Char('0') if self.state == VimState::Count => {
                self.count1 = Some(self.count1.unwrap_or(0) * 10);
                Action::None
            }

            // Operators
            KeyCode::Char('d') => {
                self.state =
                    VimState::OperatorPending { operator: Operator::Delete };
                Action::None
            }
            KeyCode::Char('c') => {
                self.state =
                    VimState::OperatorPending { operator: Operator::Change };
                Action::None
            }
            KeyCode::Char('y') => {
                self.state =
                    VimState::OperatorPending { operator: Operator::Yank };
                Action::None
            }

            // g-prefix
            KeyCode::Char('g') => {
                self.state = VimState::WaitingG;
                Action::None
            }

            // Angle brackets (indent/dedent or
            // operator)
            KeyCode::Char('>') => {
                self.state = VimState::WaitingAngle { is_indent: true };
                Action::None
            }
            KeyCode::Char('<') => {
                self.state = VimState::WaitingAngle { is_indent: false };
                Action::None
            }

            // Find char motions
            KeyCode::Char('f') => {
                self.state =
                    VimState::WaitingChar { purpose: CharPurpose::FindChar };
                Action::None
            }
            KeyCode::Char('F') => {
                self.state = VimState::WaitingChar {
                    purpose: CharPurpose::FindCharBack,
                };
                Action::None
            }
            KeyCode::Char('t') => {
                self.state =
                    VimState::WaitingChar { purpose: CharPurpose::TillChar };
                Action::None
            }
            KeyCode::Char('T') => {
                self.state = VimState::WaitingChar {
                    purpose: CharPurpose::TillCharBack,
                };
                Action::None
            }

            // Replace char
            KeyCode::Char('r') => {
                self.state = VimState::WaitingChar {
                    purpose: CharPurpose::ReplaceChar,
                };
                Action::None
            }

            // Motions
            KeyCode::Char(c) => {
                if let Some(m) = self.char_to_motion(c) {
                    self.emit_and_reset(Action::Move(m, self.count1_val()))
                } else if let Some(a) = self.char_to_special(c) {
                    a
                } else if let Some(v) = self.char_to_insert(c) {
                    self.emit_and_reset(Action::EnterInsert(v))
                } else if c == ':' {
                    self.emit_and_reset(Action::EnterCommand)
                } else {
                    self.reset();
                    Action::None
                }
            }

            // Arrow keys as motions
            KeyCode::Left => self
                .emit_and_reset(Action::Move(Motion::Left, self.count1_val())),
            KeyCode::Right => self.emit_and_reset(Action::Move(
                Motion::Right,
                self.count1_val(),
            )),
            KeyCode::Up => self
                .emit_and_reset(Action::Move(Motion::Up, self.count1_val())),
            KeyCode::Down => self
                .emit_and_reset(Action::Move(Motion::Down, self.count1_val())),

            _ => {
                self.reset();
                Action::None
            }
        }
    }

    // ── Operator pending ──────────────────────────

    #[allow(clippy::too_many_lines)]
    fn feed_operator_pending(
        &mut self,
        key: KeyInput,
        operator: Operator,
    ) -> Action {
        if key.ctrl {
            self.reset();
            return Action::None;
        }

        match key.code {
            // Second count
            KeyCode::Char(c @ '1'..='9') => {
                let d = c as usize - '0' as usize;
                self.count2 = Some(self.count2.unwrap_or(0) * 10 + d);
                self.state = VimState::OperatorCount { operator };
                Action::None
            }

            // Double operator: dd/cc/yy
            KeyCode::Char('d') if operator == Operator::Delete => self
                .emit_and_reset(Action::OperatorLine {
                    operator,
                    count: self.count1_val(),
                }),
            KeyCode::Char('c') if operator == Operator::Change => self
                .emit_and_reset(Action::OperatorLine {
                    operator,
                    count: self.count1_val(),
                }),
            KeyCode::Char('y') if operator == Operator::Yank => self
                .emit_and_reset(Action::OperatorLine {
                    operator,
                    count: self.count1_val(),
                }),

            // Find char inside operator
            KeyCode::Char('f') => {
                self.state = VimState::WaitingChar {
                    purpose: CharPurpose::OpFindChar { operator },
                };
                Action::None
            }
            KeyCode::Char('F') => {
                self.state = VimState::WaitingChar {
                    purpose: CharPurpose::OpFindCharBack { operator },
                };
                Action::None
            }
            KeyCode::Char('t') => {
                self.state = VimState::WaitingChar {
                    purpose: CharPurpose::OpTillChar { operator },
                };
                Action::None
            }
            KeyCode::Char('T') => {
                self.state = VimState::WaitingChar {
                    purpose: CharPurpose::OpTillCharBack { operator },
                };
                Action::None
            }

            // g prefix in operator-pending
            KeyCode::Char('g') => {
                self.state = VimState::WaitingGOp { operator };
                Action::None
            }

            // Motion key
            KeyCode::Char(c) => {
                if let Some(m) = self.char_to_motion(c) {
                    self.emit_and_reset(Action::OperatorMotion {
                        operator,
                        motion: m,
                        count: self.effective_count(),
                    })
                } else {
                    self.reset();
                    Action::None
                }
            }

            _ => {
                self.reset();
                Action::None
            }
        }
    }

    // ── Operator count ────────────────────────────

    fn feed_operator_count(
        &mut self,
        key: KeyInput,
        operator: Operator,
    ) -> Action {
        match key.code {
            KeyCode::Char(c @ '0'..='9') => {
                let d = c as usize - '0' as usize;
                self.count2 = Some(self.count2.unwrap_or(0) * 10 + d);
                Action::None
            }
            KeyCode::Char(c) => {
                if let Some(m) = self.char_to_motion(c) {
                    self.emit_and_reset(Action::OperatorMotion {
                        operator,
                        motion: m,
                        count: self.effective_count(),
                    })
                } else {
                    self.reset();
                    Action::None
                }
            }
            _ => {
                self.reset();
                Action::None
            }
        }
    }

    // ── Waiting for char ──────────────────────────

    fn feed_waiting_char(
        &mut self,
        key: KeyInput,
        purpose: CharPurpose,
    ) -> Action {
        let KeyCode::Char(c) = key.code else {
            self.reset();
            return Action::None;
        };

        let count = self.effective_count();
        match purpose {
            CharPurpose::FindChar => {
                self.emit_and_reset(Action::Move(Motion::FindChar(c), count))
            }
            CharPurpose::FindCharBack => self
                .emit_and_reset(Action::Move(Motion::FindCharBack(c), count)),
            CharPurpose::TillChar => {
                self.emit_and_reset(Action::Move(Motion::TillChar(c), count))
            }
            CharPurpose::TillCharBack => self
                .emit_and_reset(Action::Move(Motion::TillCharBack(c), count)),
            CharPurpose::ReplaceChar => self.emit_and_reset(Action::Special(
                SpecialCommand::ReplaceChar(c),
                count,
            )),
            CharPurpose::OpFindChar { operator } => {
                self.emit_and_reset(Action::OperatorMotion {
                    operator,
                    motion: Motion::FindChar(c),
                    count,
                })
            }
            CharPurpose::OpFindCharBack { operator } => {
                self.emit_and_reset(Action::OperatorMotion {
                    operator,
                    motion: Motion::FindCharBack(c),
                    count,
                })
            }
            CharPurpose::OpTillChar { operator } => {
                self.emit_and_reset(Action::OperatorMotion {
                    operator,
                    motion: Motion::TillChar(c),
                    count,
                })
            }
            CharPurpose::OpTillCharBack { operator } => {
                self.emit_and_reset(Action::OperatorMotion {
                    operator,
                    motion: Motion::TillCharBack(c),
                    count,
                })
            }
        }
    }

    // ── g-prefix ──────────────────────────────────

    fn feed_waiting_g(&mut self, key: KeyInput) -> Action {
        match key.code {
            KeyCode::Char('g') => self.emit_and_reset(Action::Move(
                Motion::GotoTop,
                self.count1_val(),
            )),
            KeyCode::Char('u') => {
                self.state = VimState::OperatorPending {
                    operator: Operator::Lowercase,
                };
                Action::None
            }
            KeyCode::Char('U') => {
                self.state = VimState::OperatorPending {
                    operator: Operator::Uppercase,
                };
                Action::None
            }
            KeyCode::Char('~') => {
                self.state = VimState::OperatorPending {
                    operator: Operator::ToggleCase,
                };
                Action::None
            }
            _ => {
                self.reset();
                Action::None
            }
        }
    }

    fn feed_waiting_g_op(
        &mut self,
        key: KeyInput,
        operator: Operator,
    ) -> Action {
        if key.code == KeyCode::Char('g') {
            self.emit_and_reset(Action::OperatorMotion {
                operator,
                motion: Motion::GotoTop,
                count: self.effective_count(),
            })
        } else {
            self.reset();
            Action::None
        }
    }

    // ── Angle bracket (>>/<<) ─────────────────────

    fn feed_waiting_angle(
        &mut self,
        key: KeyInput,
        is_indent: bool,
    ) -> Action {
        match key.code {
            KeyCode::Char('>') if is_indent => self.emit_and_reset(
                Action::Special(SpecialCommand::IndentLine, self.count1_val()),
            ),
            KeyCode::Char('<') if !is_indent => self.emit_and_reset(
                Action::Special(SpecialCommand::DedentLine, self.count1_val()),
            ),
            // > or < as operator with motion
            KeyCode::Char(c) => {
                let op = if is_indent {
                    Operator::Indent
                } else {
                    Operator::Dedent
                };
                if let Some(m) = self.char_to_motion(c) {
                    self.emit_and_reset(Action::OperatorMotion {
                        operator: op,
                        motion: m,
                        count: self.effective_count(),
                    })
                } else {
                    self.state = VimState::OperatorPending { operator: op };
                    // Re-feed the key
                    self.feed_operator_pending(key, op)
                }
            }
            _ => {
                self.reset();
                Action::None
            }
        }
    }

    fn feed_waiting_angle_op(
        &mut self,
        key: KeyInput,
        operator: Operator,
    ) -> Action {
        self.feed_operator_pending(key, operator)
    }

    // ── Helpers ───────────────────────────────────

    #[allow(clippy::unused_self)]
    const fn char_to_motion(&self, c: char) -> Option<Motion> {
        match c {
            'h' => Some(Motion::Left),
            'l' => Some(Motion::Right),
            'j' => Some(Motion::Down),
            'k' => Some(Motion::Up),
            'w' => Some(Motion::WordStart),
            'W' => Some(Motion::WordStartBig),
            'e' => Some(Motion::WordEnd),
            'E' => Some(Motion::WordEndBig),
            'b' => Some(Motion::BackWord),
            'B' => Some(Motion::BackWordBig),
            '0' => Some(Motion::LineStart),
            '$' => Some(Motion::LineEnd),
            '^' => Some(Motion::FirstNonBlank),
            'G' => Some(Motion::GotoBottom),
            '{' => Some(Motion::ParagraphBackward),
            '}' => Some(Motion::ParagraphForward),
            '%' => Some(Motion::MatchBracket),
            'H' => Some(Motion::ScreenTop),
            'M' => Some(Motion::ScreenMiddle),
            'L' => Some(Motion::ScreenBottom),
            _ => None,
        }
    }

    fn char_to_special(&mut self, c: char) -> Option<Action> {
        let count = self.count1_val();
        let action = match c {
            'x' => Action::Special(SpecialCommand::DeleteChar, count),
            'X' => Action::Special(SpecialCommand::DeleteCharBefore, count),
            's' => Action::Special(SpecialCommand::Substitute, count),
            'S' => Action::Special(SpecialCommand::SubstituteLine, count),
            'J' => Action::Special(SpecialCommand::JoinLines, count),
            '~' => Action::Special(SpecialCommand::ToggleCaseChar, count),
            'C' => Action::Special(SpecialCommand::ChangeToEnd, count),
            'D' => Action::Special(SpecialCommand::DeleteToEnd, count),
            'Y' => Action::Special(SpecialCommand::YankLine, count),
            'p' => Action::Special(SpecialCommand::Paste, count),
            'P' => Action::Special(SpecialCommand::PasteBefore, count),
            'u' => Action::Special(SpecialCommand::Undo, count),
            '.' => Action::Special(SpecialCommand::DotRepeat, count),
            _ => return None,
        };
        Some(self.emit_and_reset(action))
    }

    #[allow(clippy::unused_self)]
    const fn char_to_insert(&self, c: char) -> Option<InsertVariant> {
        match c {
            'i' => Some(InsertVariant::Before),
            'a' => Some(InsertVariant::After),
            'A' => Some(InsertVariant::LineEnd),
            'I' => Some(InsertVariant::LineStart),
            'o' => Some(InsertVariant::LineBelow),
            'O' => Some(InsertVariant::LineAbove),
            _ => None,
        }
    }
}

impl Default for VimMachine {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: char) -> KeyInput {
        KeyInput { code: KeyCode::Char(c), ctrl: false }
    }

    fn esc() -> KeyInput {
        KeyInput { code: KeyCode::Esc, ctrl: false }
    }

    fn ctrl(c: char) -> KeyInput {
        KeyInput { code: KeyCode::Char(c), ctrl: true }
    }

    // ── Simple motions ────────────────────────────

    #[test]
    fn motion_j() {
        let mut m = VimMachine::new();
        assert_eq!(m.feed(key('j')), Action::Move(Motion::Down, 1),);
    }

    #[test]
    fn motion_dollar() {
        let mut m = VimMachine::new();
        assert_eq!(m.feed(key('$')), Action::Move(Motion::LineEnd, 1),);
    }

    // ── Count + motion ────────────────────────────

    #[test]
    fn count_motion() {
        let mut m = VimMachine::new();
        assert_eq!(m.feed(key('3')), Action::None);
        assert_eq!(m.feed(key('j')), Action::Move(Motion::Down, 3),);
    }

    #[test]
    fn multi_digit_count() {
        let mut m = VimMachine::new();
        m.feed(key('1'));
        m.feed(key('2'));
        assert_eq!(m.feed(key('w')), Action::Move(Motion::WordStart, 12),);
    }

    #[test]
    fn zero_is_line_start_not_count() {
        let mut m = VimMachine::new();
        assert_eq!(m.feed(key('0')), Action::Move(Motion::LineStart, 1),);
    }

    #[test]
    fn zero_in_count_is_digit() {
        let mut m = VimMachine::new();
        m.feed(key('1'));
        m.feed(key('0'));
        assert_eq!(m.feed(key('j')), Action::Move(Motion::Down, 10),);
    }

    // ── Operator + motion ─────────────────────────

    #[test]
    fn delete_word() {
        let mut m = VimMachine::new();
        assert_eq!(m.feed(key('d')), Action::None);
        assert_eq!(
            m.feed(key('w')),
            Action::OperatorMotion {
                operator: Operator::Delete,
                motion: Motion::WordStart,
                count: 1,
            },
        );
    }

    #[test]
    fn change_line_end() {
        let mut m = VimMachine::new();
        m.feed(key('c'));
        assert_eq!(
            m.feed(key('$')),
            Action::OperatorMotion {
                operator: Operator::Change,
                motion: Motion::LineEnd,
                count: 1,
            },
        );
    }

    #[test]
    fn yank_word() {
        let mut m = VimMachine::new();
        m.feed(key('y'));
        assert_eq!(
            m.feed(key('w')),
            Action::OperatorMotion {
                operator: Operator::Yank,
                motion: Motion::WordStart,
                count: 1,
            },
        );
    }

    // ── Count + operator + motion ─────────────────

    #[test]
    fn count_op_motion() {
        let mut m = VimMachine::new();
        m.feed(key('2'));
        m.feed(key('d'));
        assert_eq!(
            m.feed(key('w')),
            Action::OperatorMotion {
                operator: Operator::Delete,
                motion: Motion::WordStart,
                count: 2,
            },
        );
    }

    // ── Operator + count + motion ─────────────────

    #[test]
    fn op_count_motion() {
        let mut m = VimMachine::new();
        m.feed(key('d'));
        m.feed(key('2'));
        assert_eq!(
            m.feed(key('w')),
            Action::OperatorMotion {
                operator: Operator::Delete,
                motion: Motion::WordStart,
                count: 2,
            },
        );
    }

    // ── Count * count multiplication ──────────────

    #[test]
    fn count_op_count_motion() {
        let mut m = VimMachine::new();
        m.feed(key('2'));
        m.feed(key('d'));
        m.feed(key('3'));
        assert_eq!(
            m.feed(key('w')),
            Action::OperatorMotion {
                operator: Operator::Delete,
                motion: Motion::WordStart,
                count: 6,
            },
        );
    }

    // ── Double operator (dd/cc/yy) ────────────────

    #[test]
    fn dd() {
        let mut m = VimMachine::new();
        m.feed(key('d'));
        assert_eq!(
            m.feed(key('d')),
            Action::OperatorLine { operator: Operator::Delete, count: 1 },
        );
    }

    #[test]
    fn three_dd() {
        let mut m = VimMachine::new();
        m.feed(key('3'));
        m.feed(key('d'));
        assert_eq!(
            m.feed(key('d')),
            Action::OperatorLine { operator: Operator::Delete, count: 3 },
        );
    }

    #[test]
    fn yy() {
        let mut m = VimMachine::new();
        m.feed(key('y'));
        assert_eq!(
            m.feed(key('y')),
            Action::OperatorLine { operator: Operator::Yank, count: 1 },
        );
    }

    #[test]
    fn cc() {
        let mut m = VimMachine::new();
        m.feed(key('c'));
        assert_eq!(
            m.feed(key('c')),
            Action::OperatorLine { operator: Operator::Change, count: 1 },
        );
    }

    // ── g-prefix ──────────────────────────────────

    #[test]
    fn gg() {
        let mut m = VimMachine::new();
        m.feed(key('g'));
        assert_eq!(m.feed(key('g')), Action::Move(Motion::GotoTop, 1),);
    }

    #[test]
    fn gu_word() {
        let mut m = VimMachine::new();
        m.feed(key('g'));
        m.feed(key('u'));
        assert_eq!(
            m.feed(key('w')),
            Action::OperatorMotion {
                operator: Operator::Lowercase,
                motion: Motion::WordStart,
                count: 1,
            },
        );
    }

    #[test]
    fn g_upper_u_word() {
        let mut m = VimMachine::new();
        m.feed(key('g'));
        m.feed(key('U'));
        assert_eq!(
            m.feed(key('w')),
            Action::OperatorMotion {
                operator: Operator::Uppercase,
                motion: Motion::WordStart,
                count: 1,
            },
        );
    }

    // ── >>/<<  ────────────────────────────────────

    #[test]
    fn indent() {
        let mut m = VimMachine::new();
        m.feed(key('>'));
        assert_eq!(
            m.feed(key('>')),
            Action::Special(SpecialCommand::IndentLine, 1,),
        );
    }

    #[test]
    fn dedent() {
        let mut m = VimMachine::new();
        m.feed(key('<'));
        assert_eq!(
            m.feed(key('<')),
            Action::Special(SpecialCommand::DedentLine, 1,),
        );
    }

    // ── Find char ─────────────────────────────────

    #[test]
    fn find_char() {
        let mut m = VimMachine::new();
        m.feed(key('f'));
        assert_eq!(m.feed(key('a')), Action::Move(Motion::FindChar('a'), 1),);
    }

    #[test]
    fn delete_find_char() {
        let mut m = VimMachine::new();
        m.feed(key('d'));
        m.feed(key('f'));
        assert_eq!(
            m.feed(key('a')),
            Action::OperatorMotion {
                operator: Operator::Delete,
                motion: Motion::FindChar('a'),
                count: 1,
            },
        );
    }

    // ── Replace char ──────────────────────────────

    #[test]
    fn replace_char() {
        let mut m = VimMachine::new();
        m.feed(key('r'));
        assert_eq!(
            m.feed(key('x')),
            Action::Special(SpecialCommand::ReplaceChar('x'), 1,),
        );
    }

    // ── Special commands ──────────────────────────

    #[test]
    fn delete_char_x() {
        let mut m = VimMachine::new();
        assert_eq!(
            m.feed(key('x')),
            Action::Special(SpecialCommand::DeleteChar, 1,),
        );
    }

    #[test]
    fn undo() {
        let mut m = VimMachine::new();
        assert_eq!(
            m.feed(key('u')),
            Action::Special(SpecialCommand::Undo, 1,),
        );
    }

    #[test]
    fn redo() {
        let mut m = VimMachine::new();
        assert_eq!(
            m.feed(ctrl('r')),
            Action::Special(SpecialCommand::Redo, 1,),
        );
    }

    #[test]
    fn paste() {
        let mut m = VimMachine::new();
        assert_eq!(
            m.feed(key('p')),
            Action::Special(SpecialCommand::Paste, 1,),
        );
    }

    // ── Insert variants ───────────────────────────

    #[test]
    fn insert_i() {
        let mut m = VimMachine::new();
        assert_eq!(
            m.feed(key('i')),
            Action::EnterInsert(InsertVariant::Before),
        );
    }

    #[test]
    fn insert_a() {
        let mut m = VimMachine::new();
        assert_eq!(
            m.feed(key('a')),
            Action::EnterInsert(InsertVariant::After),
        );
    }

    #[test]
    fn insert_o() {
        let mut m = VimMachine::new();
        assert_eq!(
            m.feed(key('o')),
            Action::EnterInsert(InsertVariant::LineBelow),
        );
    }

    // ── Esc cancels ───────────────────────────────

    #[test]
    fn esc_cancels_operator() {
        let mut m = VimMachine::new();
        m.feed(key('d'));
        assert_eq!(m.feed(esc()), Action::None);
        // Next key should be normal
        assert_eq!(m.feed(key('j')), Action::Move(Motion::Down, 1),);
    }

    #[test]
    fn esc_cancels_count() {
        let mut m = VimMachine::new();
        m.feed(key('3'));
        m.feed(esc());
        assert_eq!(m.feed(key('j')), Action::Move(Motion::Down, 1),);
    }

    // ── Command mode ──────────────────────────────

    #[test]
    fn enter_command() {
        let mut m = VimMachine::new();
        assert_eq!(m.feed(key(':')), Action::EnterCommand,);
    }

    // ── dgg (operator + g-prefix motion) ──────────

    #[test]
    fn delete_to_top() {
        let mut m = VimMachine::new();
        m.feed(key('d'));
        m.feed(key('g'));
        assert_eq!(
            m.feed(key('g')),
            Action::OperatorMotion {
                operator: Operator::Delete,
                motion: Motion::GotoTop,
                count: 1,
            },
        );
    }
}
