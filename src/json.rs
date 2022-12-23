// Copyright (c) 2005, 2022 Atif Aziz. All rights reserved.
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{error::Error, fmt::Display};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SyntaxError {
    UnclosedComment,
    MissingValue,
    UnterminatedString,
    UnterminatedArray,
    UnterminatedObject,
    InvalidMemberValueDelimiter,
}

impl Display for SyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Error for SyntaxError {}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct JsonToken<'s> {
    kind: JsonTokenKind,
    text: &'s str,
}

impl<'s> JsonToken<'s> {
    fn new(kind: JsonTokenKind, text: &'s str) -> Self {
        Self { kind, text }
    }

    pub fn kind(&self) -> JsonTokenKind {
        self.kind
    }

    pub fn text(&self) -> &'s str {
        self.text
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum JsonTokenKind {
    Null,
    True,
    False,
    Number,
    String,
    ArrayStart,
    ArrayEnd,
    ObjectStart,
    ObjectEnd,
    ObjectMember,
}

type IdxChar = (usize, char);

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ReaderState {
    Parse,
    ParseArrayFirst,
    ParseArrayNext,
    ParseObjectMemberName,
    ParseObjectMemberValue,
    ParseObjectMemberNext,
}

#[derive(Debug)]
pub struct JsonTextReader<'s> {
    source: &'s str,
    state_stack: Vec<ReaderState>,
    idx_chars: std::str::CharIndices<'s>,
    next: Option<IdxChar>,
}

impl<'s> Iterator for JsonTextReader<'s> {
    type Item = Result<JsonToken<'s>, SyntaxError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.state_stack.pop().map(|state| match state {
            ReaderState::Parse => self.parse(),
            ReaderState::ParseArrayFirst => self.parse_array_first(),
            ReaderState::ParseArrayNext => self.parse_array_next(),
            ReaderState::ParseObjectMemberName => self.parse_object_member_name(),
            ReaderState::ParseObjectMemberValue => self.parse_object_member_value(),
            ReaderState::ParseObjectMemberNext => self.parse_object_member_next(),
        })
    }
}

impl<'s> JsonTextReader<'s> {
    pub fn new(source: &'s str) -> Self {
        Self {
            source,
            idx_chars: source.char_indices(),
            next: None,
            state_stack: vec![ReaderState::Parse],
        }
    }

    fn next(&mut self) -> Option<IdxChar> {
        if let Some(next) = self.next {
            self.next = None;
            Some(next)
        } else {
            self.idx_chars.next()
        }
    }

    fn back(&mut self, ich: IdxChar) {
        self.next = Some(ich)
    }

    fn parse_object_member_name(&mut self) -> Result<JsonToken<'s>, SyntaxError> {
        let ich = self.next_clean()?.ok_or(SyntaxError::UnterminatedObject)?;
        if let (i, '}') = ich {
            Ok(JsonToken::new(JsonTokenKind::ObjectEnd, &self.source[i..=i]))
        } else {
            self.back(ich);
            self.state_stack.push(ReaderState::ParseObjectMemberValue);
            self.parse().map(|token| JsonToken::new(JsonTokenKind::ObjectMember, token.text()))
        }
    }

    fn parse_object_member_value(&mut self) -> Result<JsonToken<'s>, SyntaxError> {
        let ich = self.next_clean()?.ok_or(SyntaxError::UnterminatedObject)?;
        match ich {
            (_, ':') => {}
            (_, '=') => {
                let ich = self.next().ok_or(SyntaxError::InvalidMemberValueDelimiter)?;
                if let (_, '>') = ich {
                    self.back(ich);
                }
            }
            _ => Err(SyntaxError::InvalidMemberValueDelimiter)?,
        }
        self.state_stack.push(ReaderState::ParseObjectMemberNext);
        self.parse()
    }

    fn parse_object_member_next(&mut self) -> Result<JsonToken<'s>, SyntaxError> {
        let ich = self.next_clean()?.ok_or(SyntaxError::UnterminatedObject)?;
        match ich {
            (_, ';' | ',') => {
                let ich = self.next_clean()?.ok_or(SyntaxError::UnterminatedObject)?;
                if let (i, '}') = ich {
                    Ok(JsonToken::new(JsonTokenKind::ObjectEnd, &self.source[i..=i]))
                } else {
                    self.back(ich);
                    self.state_stack.push(ReaderState::ParseObjectMemberValue);
                    self.parse()
                }
            }
            (i, '}') => Ok(JsonToken::new(JsonTokenKind::ObjectEnd, &self.source[i..=i])),
            _ => Err(SyntaxError::UnterminatedObject),
        }
    }

    fn parse_array_first(&mut self) -> Result<JsonToken<'s>, SyntaxError> {
        let ich = self.next_clean()?.ok_or(SyntaxError::UnterminatedArray)?;
        if let (i, ']') = ich {
            Ok(JsonToken::new(JsonTokenKind::ArrayEnd, &self.source[i..=i]))
        } else {
            self.back(ich);
            self.state_stack.push(ReaderState::ParseArrayNext);
            self.parse()
        }
    }

    fn parse_array_next(&mut self) -> Result<JsonToken<'s>, SyntaxError> {
        let ich = self.next_clean()?.ok_or(SyntaxError::UnterminatedArray)?;
        match ich {
            (_, ',' | ';') => {
                let ich = self.next_clean()?.ok_or(SyntaxError::UnterminatedArray)?;
                if let (i, ']') = ich {
                    Ok(JsonToken::new(JsonTokenKind::ArrayEnd, &self.source[i..=i]))
                } else {
                    self.back(ich);
                    self.state_stack.push(ReaderState::ParseArrayNext);
                    self.parse()
                }
            }
            (i, ']') => Ok(JsonToken::new(JsonTokenKind::ArrayEnd, &self.source[i..=i])),
            _ => Err(SyntaxError::UnterminatedArray),
        }
    }

    fn parse_string(&mut self, quote: IdxChar) -> Result<&'s str, SyntaxError> {
        let (si, quote) = quote;
        loop {
            match self.next() {
                None | Some((_, '\n')) | Some((_, '\r')) => Err(SyntaxError::UnterminatedString)?,
                Some((_, '\\')) if self.next().is_none() => Err(SyntaxError::UnterminatedString)?,
                Some((ei, ch)) if ch == quote => return Ok(&self.source[si..=ei]),
                _ => {}
            }
        }
    }

    fn parse(&mut self) -> Result<JsonToken<'s>, SyntaxError> {
        let ich = self.next_clean()?.ok_or(SyntaxError::MissingValue)?;
        Ok(match ich {
            // String
            ich @ (_, ch) if ch == '"' || ch == '\'' => {
                JsonToken::new(JsonTokenKind::String, self.parse_string(ich)?)
            }
            (i, '{') => {
                self.state_stack.push(ReaderState::ParseObjectMemberName);
                JsonToken::new(JsonTokenKind::ObjectStart, &self.source[i..=i])
            }
            (i, '[') => {
                self.state_stack.push(ReaderState::ParseArrayFirst);
                JsonToken::new(JsonTokenKind::ArrayStart, &self.source[i..=i])
            }
            (si, mut ch) => {
                //
                // Handle unquoted text. This could be the values true, false, or
                // null, or it can be a number. An implementation (such as this one)
                // is allowed to also accept non-standard forms.
                //
                // Accumulate characters until we reach the end of the text or a
                // formatting character.
                //
                let mut ei = si;
                let mut pi = si;
                loop {
                    if ch >= ' ' && ",:]}/\\\"[{;=#".find(ch).is_none() {
                        ei += ch.len_utf8();
                        if let Some(next) = self.next() {
                            (pi, ch) = next;
                        } else {
                            break;
                        }
                    } else {
                        self.back((pi, ch));
                        break;
                    }
                }

                if ei == si {
                    Err(SyntaxError::MissingValue)?;
                }

                let tt = &self.source[si..ei];
                let kind = match tt {
                    "null" => JsonTokenKind::Null,
                    "true" => JsonTokenKind::True,
                    "false" => JsonTokenKind::False,
                    other => {
                        //
                        // Try converting it. We support the 0- and 0x- conventions.
                        // If a number cannot be produced, then the value will just
                        // be a string. Note that the 0-, 0x-, plus, and implied
                        // string conventions are non-standard, but a JSON text parser
                        // is free to accept non-JSON text forms as long as it accepts
                        // all correct JSON text forms.
                        //
                        if other.trim_end().parse::<f64>().is_ok() {
                            JsonTokenKind::Number
                        } else {
                            JsonTokenKind::String
                        }
                    }
                };

                JsonToken::new(kind, tt)
            }
        })
    }

    fn next_clean(&mut self) -> Result<Option<IdxChar>, SyntaxError> {
        loop {
            let Some(ich @ (_, ch)) = self.next() else {
                return Ok(None)
            };
            match ch {
                '/' => {
                    match self.next() {
                        None => return Ok(Some(ich)),
                        //
                        // Single-line comment: // ...
                        //
                        Some((_, '/')) => {
                            while let Some((_, ch)) = self.next() {
                                if let '\n' | '\r' = ch {
                                    break;
                                }
                            }
                        }
                        //
                        // Multi-line comment: /* ... */
                        //
                        Some((_, '*')) => loop {
                            let Some((_, ch)) = self.next() else {
                                return Err(SyntaxError::UnclosedComment);
                            };

                            if ch == '*' {
                                match self.next() {
                                    Some((_, '/')) => break,
                                    Some(ich) => self.back(ich),
                                    _ => return Err(SyntaxError::UnclosedComment),
                                };
                            }
                        },
                        Some(ich) => {
                            self.back(ich);
                            return Ok(Some(ich));
                        }
                    }
                }
                '#' => {
                    while let Some((_, ch)) = self.next() {
                        if ch == '\n' || ch == '\r' {
                            break;
                        }
                    }
                }
                ch if ch > ' ' => return Ok(Some(ich)),
                _ => continue,
            }
        }
    }
}
