//! A reader for .NET `BinaryFormatter` streams ([MS-NRBF]), enough for the files
//! LaserGRBL writes: classes with and without member types, strings, primitive and
//! object arrays, references and nulls. Read only: the stream becomes a tree of
//! [`Value`]s with references resolved; nothing is ever instantiated or executed.
//!
//! [MS-NRBF]: https://learn.microsoft.com/openspecs/windows_protocols/ms-nrbf/

use std::collections::HashMap;

/// Errors of the reader.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NrbfError {
    /// The stream ended early.
    #[error("unexpected end of data at byte {0}")]
    Eof(usize),
    /// A record or type this reader does not know.
    #[error("unsupported record or type {0} at byte {1}")]
    Unsupported(u8, usize),
    /// Malformed data.
    #[error("invalid data: {0}")]
    Invalid(String),
}

/// A deserialized value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `null`.
    Null,
    /// `bool`.
    Bool(bool),
    /// Any signed integer.
    Int(i64),
    /// Any unsigned integer.
    UInt(u64),
    /// `float` or `double`.
    Float(f64),
    /// `decimal`, as its invariant text.
    Decimal(String),
    /// `char`.
    Char(char),
    /// `string`.
    String(String),
    /// `TimeSpan`, in 100 ns ticks.
    TimeSpan(i64),
    /// `DateTime`: ticks since 0001-01-01 and the kind (0 unspecified, 1 UTC, 2 local).
    DateTime {
        /// 100 ns ticks.
        ticks: i64,
        /// `DateTimeKind`.
        kind: u8,
    },
    /// An object: its class name and members in order.
    Object {
        /// Full class name (`System.Collections.Generic.Dictionary\`2[...]`).
        class: String,
        /// Members, in serialization order.
        members: Vec<(String, Value)>,
    },
    /// An array.
    Array(Vec<Value>),
}

impl Value {
    /// The member `name` of an object (exact, or a base-class member `Base+name`).
    #[must_use]
    pub fn member(&self, name: &str) -> Option<&Value> {
        match self {
            Self::Object { members, .. } => members
                .iter()
                .find(|(n, _)| n == name || n.rsplit('+').next() == Some(name))
                .map(|(_, v)| v),
            _ => None,
        }
    }

    /// The class name of an object.
    #[must_use]
    pub fn class(&self) -> Option<&str> {
        match self {
            Self::Object { class, .. } => Some(class),
            _ => None,
        }
    }

    /// The value as text (strings, chars, decimals).
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) | Self::Decimal(s) => Some(s),
            _ => None,
        }
    }

    /// The value as an integer; an enum (an object with `value__`) gives its value.
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(v) => Some(*v),
            Self::UInt(v) => i64::try_from(*v).ok(),
            Self::Object { .. } => self.member("value__").and_then(Self::as_i64),
            _ => None,
        }
    }

    /// The value as a number (integers, floats, decimals, enums).
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(v) => Some(*v),
            Self::Decimal(s) => s.parse().ok(),
            _ => self.as_i64().map(|v| v as f64),
        }
    }

    /// The value as a boolean.
    #[must_use]
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// The elements of an array.
    #[must_use]
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Self::Array(a) => Some(a),
            _ => None,
        }
    }
}

/// `BinaryTypeEnum` with its additional information.
#[derive(Debug, Clone, PartialEq)]
enum MemberType {
    Primitive(u8),
    String,
    Object,
    SystemClass,
    Class,
    ObjectArray,
    StringArray,
    PrimitiveArray(u8),
}

#[derive(Debug, Clone)]
struct ClassMeta {
    name: String,
    members: Vec<String>,
    types: Option<Vec<MemberType>>,
}

/// A value while reading: references are resolved at the end.
#[derive(Debug, Clone)]
enum Raw {
    Value(Value),
    Ref(i32),
    Object {
        class: String,
        members: Vec<(String, Raw)>,
    },
    Array(Vec<Raw>),
    /// `ObjectNullMultiple`: this many nulls in an array or member list.
    Nulls(usize),
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    classes: HashMap<i32, ClassMeta>,
    objects: HashMap<i32, Raw>,
    depth: usize,
}

const MAX_DEPTH: usize = 200;

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], NrbfError> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.data.len())
            .ok_or(NrbfError::Eof(self.pos))?;
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8, NrbfError> {
        Ok(self.take(1)?[0])
    }

    fn arr<const N: usize>(&mut self) -> Result<[u8; N], NrbfError> {
        let mut a = [0u8; N];
        a.copy_from_slice(self.take(N)?);
        Ok(a)
    }

    fn i32(&mut self) -> Result<i32, NrbfError> {
        Ok(i32::from_le_bytes(self.arr()?))
    }

    fn count(&mut self) -> Result<usize, NrbfError> {
        let n = self.i32()?;
        usize::try_from(n).map_err(|_| NrbfError::Invalid(format!("negative count {n}")))
    }

    /// `LengthPrefixedString`: a 7-bit encoded length, then UTF-8.
    fn string(&mut self) -> Result<String, NrbfError> {
        let mut len = 0usize;
        for shift in (0..35).step_by(7) {
            let b = self.u8()?;
            len |= usize::from(b & 0x7F) << shift;
            if b & 0x80 == 0 {
                let bytes = self.take(len)?;
                return Ok(String::from_utf8_lossy(bytes).into_owned());
            }
        }
        Err(NrbfError::Invalid("string length too long".into()))
    }

    fn primitive(&mut self, t: u8) -> Result<Value, NrbfError> {
        let at = self.pos;
        Ok(match t {
            1 => Value::Bool(self.u8()? != 0),
            2 => Value::UInt(u64::from(self.u8()?)),
            3 => {
                // a UTF-8 character: its length from the first byte
                let first = self.u8()?;
                let extra = match first {
                    0x00..=0x7F => 0,
                    0xC0..=0xDF => 1,
                    0xE0..=0xEF => 2,
                    _ => 3,
                };
                let mut bytes = vec![first];
                bytes.extend_from_slice(self.take(extra)?);
                Value::Char(
                    String::from_utf8_lossy(&bytes)
                        .chars()
                        .next()
                        .unwrap_or('\u{fffd}'),
                )
            }
            5 => Value::Decimal(self.string()?),
            6 => Value::Float(f64::from_le_bytes(self.arr()?)),
            7 => Value::Int(i64::from(i16::from_le_bytes(self.arr()?))),
            8 => Value::Int(i64::from(self.i32()?)),
            9 => Value::Int(i64::from_le_bytes(self.arr()?)),
            10 => Value::Int(i64::from(i8::from_le_bytes(self.arr()?))),
            11 => Value::Float(f64::from(f32::from_le_bytes(self.arr()?))),
            12 => Value::TimeSpan(i64::from_le_bytes(self.arr()?)),
            13 => {
                let raw = u64::from_le_bytes(self.arr()?);
                #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                Value::DateTime {
                    ticks: (raw & 0x3FFF_FFFF_FFFF_FFFF) as i64,
                    kind: (raw >> 62) as u8,
                }
            }
            14 => Value::UInt(u64::from(u16::from_le_bytes(self.arr()?))),
            15 => Value::UInt(u64::from(u32::from_le_bytes(self.arr()?))),
            16 => Value::UInt(u64::from_le_bytes(self.arr()?)),
            17 => Value::Null,
            18 => Value::String(self.string()?),
            _ => return Err(NrbfError::Unsupported(t, at)),
        })
    }

    fn class_info(&mut self) -> Result<(i32, String, Vec<String>), NrbfError> {
        let id = self.i32()?;
        let name = self.string()?;
        let n = self.count()?;
        let mut members = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            members.push(self.string()?);
        }
        Ok((id, name, members))
    }

    fn member_types(&mut self, n: usize) -> Result<Vec<MemberType>, NrbfError> {
        let kinds: Vec<u8> = (0..n).map(|_| self.u8()).collect::<Result<_, _>>()?;
        let mut out = Vec::with_capacity(n);
        for k in kinds {
            let at = self.pos;
            out.push(match k {
                0 => MemberType::Primitive(self.u8()?),
                1 => MemberType::String,
                2 => MemberType::Object,
                3 => {
                    self.string()?;
                    MemberType::SystemClass
                }
                4 => {
                    self.string()?;
                    self.i32()?;
                    MemberType::Class
                }
                5 => MemberType::ObjectArray,
                6 => MemberType::StringArray,
                7 => MemberType::PrimitiveArray(self.u8()?),
                _ => return Err(NrbfError::Unsupported(k, at)),
            });
        }
        Ok(out)
    }

    /// The values of `count` slots (members or array elements), each a record or,
    /// for primitive slots, a raw value.
    fn slots(&mut self, types: Option<&[MemberType]>, count: usize) -> Result<Vec<Raw>, NrbfError> {
        let mut out = Vec::with_capacity(count.min(65536));
        while out.len() < count {
            let i = out.len();
            if let Some(MemberType::Primitive(t)) = types.and_then(|t| t.get(i)) {
                let t = *t;
                out.push(Raw::Value(self.primitive(t)?));
                continue;
            }
            match self.record()? {
                Some(Raw::Nulls(n)) => out.extend(std::iter::repeat_n(Raw::Value(Value::Null), n.max(1))),
                Some(r) => out.push(r),
                None => return Err(NrbfError::Invalid("message end inside an object".into())),
            }
        }
        out.truncate(count);
        Ok(out)
    }

    fn object(&mut self, id: i32, meta: &ClassMeta) -> Result<Raw, NrbfError> {
        let values = self.slots(meta.types.as_deref(), meta.members.len())?;
        let raw = Raw::Object {
            class: meta.name.clone(),
            members: meta.members.iter().cloned().zip(values).collect(),
        };
        self.objects.insert(id, raw.clone());
        Ok(raw)
    }

    fn keep(&mut self, id: i32, raw: Raw) -> Raw {
        self.objects.insert(id, raw.clone());
        raw
    }

    /// One record; `None` at `MessageEnd`.
    #[allow(clippy::too_many_lines)]
    fn record(&mut self) -> Result<Option<Raw>, NrbfError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(NrbfError::Invalid("objects nested too deep".into()));
        }
        let at = self.pos;
        let kind = self.u8()?;
        let r = match kind {
            0 => {
                // SerializationHeaderRecord: root id, header id, major, minor
                self.take(16)?;
                self.record()?
            }
            1 => {
                let id = self.i32()?;
                let meta_id = self.i32()?;
                let meta = self
                    .classes
                    .get(&meta_id)
                    .cloned()
                    .ok_or_else(|| NrbfError::Invalid(format!("unknown class metadata {meta_id}")))?;
                Some(self.object(id, &meta)?)
            }
            2 | 3 => {
                let (id, name, members) = self.class_info()?;
                if kind == 3 {
                    self.i32()?;
                }
                let meta = ClassMeta {
                    name,
                    members,
                    types: None,
                };
                self.classes.insert(id, meta.clone());
                Some(self.object(id, &meta)?)
            }
            4 | 5 => {
                let (id, name, members) = self.class_info()?;
                let types = self.member_types(members.len())?;
                if kind == 5 {
                    self.i32()?;
                }
                let meta = ClassMeta {
                    name,
                    members,
                    types: Some(types),
                };
                self.classes.insert(id, meta.clone());
                Some(self.object(id, &meta)?)
            }
            6 => {
                let id = self.i32()?;
                let s = Raw::Value(Value::String(self.string()?));
                Some(self.keep(id, s))
            }
            7 => {
                // BinaryArray
                let id = self.i32()?;
                let array_kind = self.u8()?;
                let rank = self.count()?;
                let lengths: Vec<usize> = (0..rank).map(|_| self.count()).collect::<Result<_, _>>()?;
                if matches!(array_kind, 3..=5) {
                    for _ in 0..rank {
                        self.i32()?;
                    }
                }
                let t = self.member_types(1)?;
                let total = lengths.iter().try_fold(1usize, |a, &l| a.checked_mul(l));
                let total = total
                    .filter(|&t| t <= 1 << 24)
                    .ok_or_else(|| NrbfError::Invalid("array too big".into()))?;
                let types = vec![t[0].clone(); total];
                let items = self.slots(Some(&types), total)?;
                Some(self.keep(id, Raw::Array(items)))
            }
            8 => {
                let t = self.u8()?;
                Some(Raw::Value(self.primitive(t)?))
            }
            9 => Some(Raw::Ref(self.i32()?)),
            10 => Some(Raw::Value(Value::Null)),
            11 => None,
            12 => {
                // BinaryLibrary: id and name, then the record it introduces
                self.i32()?;
                self.string()?;
                self.depth -= 1;
                return self.record();
            }
            13 => Some(Raw::Nulls(usize::from(self.u8()?))),
            14 => Some(Raw::Nulls(self.count()?)),
            15 => {
                let id = self.i32()?;
                let n = self.count()?;
                let t = self.u8()?;
                if n > 1 << 24 {
                    return Err(NrbfError::Invalid("array too big".into()));
                }
                let items = (0..n)
                    .map(|_| self.primitive(t).map(Raw::Value))
                    .collect::<Result<_, _>>()?;
                Some(self.keep(id, Raw::Array(items)))
            }
            16 | 17 => {
                let id = self.i32()?;
                let n = self.count()?;
                if n > 1 << 24 {
                    return Err(NrbfError::Invalid("array too big".into()));
                }
                let items = self.slots(None, n)?;
                Some(self.keep(id, Raw::Array(items)))
            }
            _ => return Err(NrbfError::Unsupported(kind, at)),
        };
        self.depth -= 1;
        Ok(r)
    }

    fn resolve(&self, raw: &Raw, depth: usize) -> Value {
        if depth > MAX_DEPTH {
            return Value::Null;
        }
        match raw {
            Raw::Value(v) => v.clone(),
            Raw::Nulls(_) => Value::Null,
            Raw::Ref(id) => self
                .objects
                .get(id)
                .map_or(Value::Null, |r| self.resolve(r, depth + 1)),
            Raw::Array(items) => Value::Array(items.iter().map(|r| self.resolve(r, depth + 1)).collect()),
            Raw::Object { class, members } => Value::Object {
                class: class.clone(),
                members: members
                    .iter()
                    .map(|(n, r)| (n.clone(), self.resolve(r, depth + 1)))
                    .collect(),
            },
        }
    }
}

/// Reads a `BinaryFormatter` stream: the root object with every reference resolved
/// (cycles cut at a fixed depth).
///
/// # Errors
/// Truncated or malformed data, or records this reader does not support.
pub fn read(data: &[u8]) -> Result<Value, NrbfError> {
    let mut r = Reader {
        data,
        pos: 0,
        classes: HashMap::new(),
        objects: HashMap::new(),
        depth: 0,
    };
    if data.first() != Some(&0) {
        return Err(NrbfError::Invalid("not a BinaryFormatter stream".into()));
    }
    let mut first = None;
    while let Some(raw) = r.record()? {
        r.depth = 0;
        first.get_or_insert(raw);
    }
    let root = first.ok_or_else(|| NrbfError::Invalid("empty stream".into()))?;
    Ok(r.resolve(&root, 0))
}

/// The ticks of a `DateTime` as `YYYY-MM-DD` (proleptic Gregorian calendar).
#[must_use]
pub fn date(ticks: i64) -> String {
    // days since 0001-01-01 → days since 1970-01-01 → civil date (Howard Hinnant's
    // `civil_from_days`)
    let unix_days = ticks.div_euclid(864_000_000_000) - 719_162;
    let z = unix_days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        // 2024-05-06 = 638505504000000000 ticks
        assert_eq!(date(638_505_504_000_000_000), "2024-05-06");
        assert_eq!(date(0), "0001-01-01");
        assert_eq!(date(630_822_816_000_000_000), "2000-01-01");
    }

    #[test]
    fn malformed_streams() {
        assert!(read(b"").is_err());
        assert!(read(b"\x01").is_err());
        assert!(matches!(read(&[0, 1, 2]), Err(NrbfError::Eof(_))));
        // header then MessageEnd: empty
        let mut s = vec![0u8];
        s.extend_from_slice(&[0; 16]);
        s.push(11);
        assert!(read(&s).is_err());
        // header, an unknown record
        let mut u = vec![0u8];
        u.extend_from_slice(&[0; 16]);
        u.push(99);
        assert!(matches!(read(&u), Err(NrbfError::Unsupported(99, _))));
        // header, a string object, end
        let mut o = vec![0u8];
        o.extend_from_slice(&[0; 16]);
        o.extend_from_slice(&[6, 1, 0, 0, 0, 2, b'h', b'i', 11]);
        assert_eq!(read(&o).unwrap(), Value::String("hi".into()));
    }

    /// A stream with the records LaserGRBL's files do not use.
    #[test]
    #[allow(clippy::too_many_lines)]
    fn every_record_kind() {
        fn lps(out: &mut Vec<u8>, s: &str) {
            out.push(u8::try_from(s.len()).unwrap());
            out.extend_from_slice(s.as_bytes());
        }
        let i32b = |v: i32| v.to_le_bytes();
        let mut d = vec![0u8];
        d.extend_from_slice(&[0; 16]);
        // BinaryLibrary, then ClassWithMembers (3): untyped members, all records
        d.push(12);
        d.extend_from_slice(&i32b(2));
        lps(&mut d, "Lib");
        d.push(3);
        d.extend_from_slice(&i32b(1));
        lps(&mut d, "Root");
        d.extend_from_slice(&i32b(7));
        for m in ["a", "b", "c", "d", "e", "f", "g"] {
            lps(&mut d, m);
        }
        d.extend_from_slice(&i32b(2));
        // a: MemberPrimitiveTyped of every small type
        d.extend_from_slice(&[8, 2, 200]); // byte
        // b: BinaryArray (7), rectangular 2×2 of Int16 primitives
        d.push(7);
        d.extend_from_slice(&i32b(3));
        d.push(2); // Rectangular
        d.extend_from_slice(&i32b(2));
        d.extend_from_slice(&i32b(2));
        d.extend_from_slice(&i32b(2));
        d.extend_from_slice(&[0, 7]); // BinaryType Primitive, Int16
        for v in [1i16, -2, 3, -4] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        // c: ArraySingleString (17) with an ObjectNullMultiple256 and a string
        d.push(17);
        d.extend_from_slice(&i32b(4));
        d.extend_from_slice(&i32b(3));
        d.extend_from_slice(&[13, 2]);
        d.push(6);
        d.extend_from_slice(&i32b(5));
        lps(&mut d, "z");
        // d: ArraySingleObject (16) with ObjectNullMultiple (14) of 2
        d.push(16);
        d.extend_from_slice(&i32b(6));
        d.extend_from_slice(&i32b(2));
        d.push(14);
        d.extend_from_slice(&i32b(2));
        // e: SystemClassWithMembers (2): untyped, one member
        d.push(2);
        d.extend_from_slice(&i32b(8));
        lps(&mut d, "Sys");
        d.extend_from_slice(&i32b(1));
        lps(&mut d, "x");
        d.extend_from_slice(&[8, 10, 0xFF]); // sbyte -1
        // f: ClassWithId (1) reusing class 8
        d.push(1);
        d.extend_from_slice(&i32b(9));
        d.extend_from_slice(&i32b(8));
        d.extend_from_slice(&[8, 14, 0x34, 0x12]); // uint16
        // g: a reference to the string 5
        d.push(9);
        d.extend_from_slice(&i32b(5));
        d.push(11);
        let v = read(&d).unwrap();
        assert_eq!(v.class(), Some("Root"));
        assert_eq!(v.member("a"), Some(&Value::UInt(200)));
        assert_eq!(
            v.member("b").and_then(Value::as_array).map(<[Value]>::len),
            Some(4)
        );
        assert_eq!(
            v.member("c").unwrap().as_array().unwrap(),
            &[Value::Null, Value::Null, Value::String("z".into())]
        );
        assert_eq!(
            v.member("d").unwrap().as_array().unwrap(),
            &[Value::Null, Value::Null]
        );
        assert_eq!(v.member("e").unwrap().member("x"), Some(&Value::Int(-1)));
        assert_eq!(v.member("f").unwrap().member("x"), Some(&Value::UInt(0x1234)));
        assert_eq!(v.member("g"), Some(&Value::String("z".into())));

        // primitives one by one, as single primitive arrays (15)
        for (t, bytes, want) in [
            (1u8, vec![1u8], Value::Bool(true)),
            (3, "é".as_bytes().to_vec(), Value::Char('é')),
            (3, "€".as_bytes().to_vec(), Value::Char('€')),
            (3, "😀".as_bytes().to_vec(), Value::Char('😀')),
            (15, 7u32.to_le_bytes().to_vec(), Value::UInt(7)),
            (16, 9u64.to_le_bytes().to_vec(), Value::UInt(9)),
            (17, vec![], Value::Null),
            (18, vec![1, b'q'], Value::String("q".into())),
        ] {
            let mut a = vec![0u8];
            a.extend_from_slice(&[0; 16]);
            a.push(15);
            a.extend_from_slice(&i32b(1));
            a.extend_from_slice(&i32b(1));
            a.push(t);
            a.extend_from_slice(&bytes);
            a.push(11);
            assert_eq!(read(&a).unwrap(), Value::Array(vec![want]), "type {t}");
        }
        // unsupported primitive and member types, unknown class metadata, bad sizes
        let mut bad = vec![0u8];
        bad.extend_from_slice(&[0; 16]);
        bad.extend_from_slice(&[8, 4]);
        assert!(matches!(read(&bad), Err(NrbfError::Unsupported(4, _))));
        let mut meta = vec![0u8];
        meta.extend_from_slice(&[0; 16]);
        meta.push(1);
        meta.extend_from_slice(&i32b(1));
        meta.extend_from_slice(&i32b(42));
        assert!(read(&meta).is_err());
        let mut neg = vec![0u8];
        neg.extend_from_slice(&[0; 16]);
        neg.push(16);
        neg.extend_from_slice(&i32b(1));
        neg.extend_from_slice(&i32b(-1));
        assert!(read(&neg).is_err());
        let mut huge = vec![0u8];
        huge.extend_from_slice(&[0; 16]);
        huge.push(15);
        huge.extend_from_slice(&i32b(1));
        huge.extend_from_slice(&i32b(i32::MAX));
        huge.push(8);
        assert!(read(&huge).is_err());
        let mut long = vec![0u8];
        long.extend_from_slice(&[0; 16]);
        long.push(6);
        long.extend_from_slice(&i32b(1));
        long.extend_from_slice(&[0xFF; 6]);
        assert!(read(&long).is_err());
        let mut ty = vec![0u8];
        ty.extend_from_slice(&[0; 16]);
        ty.push(4);
        ty.extend_from_slice(&i32b(1));
        lps(&mut ty, "T");
        ty.extend_from_slice(&i32b(1));
        lps(&mut ty, "m");
        ty.push(9);
        assert!(matches!(read(&ty), Err(NrbfError::Unsupported(9, _))));
        let mut end_inside = vec![0u8];
        end_inside.extend_from_slice(&[0; 16]);
        end_inside.push(16);
        end_inside.extend_from_slice(&i32b(1));
        end_inside.extend_from_slice(&i32b(1));
        end_inside.push(11);
        assert!(read(&end_inside).is_err());
        // a reference cycle is cut, not followed for ever
        let mut cyc = vec![0u8];
        cyc.extend_from_slice(&[0; 16]);
        cyc.push(16);
        cyc.extend_from_slice(&i32b(1));
        cyc.extend_from_slice(&i32b(1));
        cyc.push(9);
        cyc.extend_from_slice(&i32b(1));
        cyc.push(11);
        assert!(read(&cyc).is_ok());
    }

    #[test]
    fn value_helpers() {
        let e = Value::Object {
            class: "E".into(),
            members: vec![
                ("value__".into(), Value::Int(3)),
                ("List`1+_size".into(), Value::UInt(2)),
            ],
        };
        assert_eq!(e.as_i64(), Some(3));
        assert_eq!(e.as_f64(), Some(3.0));
        assert_eq!(e.member("_size"), Some(&Value::UInt(2)));
        assert_eq!(e.class(), Some("E"));
        assert_eq!(Value::Decimal("2.5".into()).as_f64(), Some(2.5));
        assert_eq!(Value::Decimal("2.5".into()).as_str(), Some("2.5"));
        assert_eq!(Value::UInt(7).as_i64(), Some(7));
        assert_eq!(Value::Bool(true).as_bool(), Some(true));
        assert_eq!(Value::Null.as_bool(), None);
        assert_eq!(Value::Null.member("x"), None);
        assert_eq!(Value::Null.class(), None);
        assert!(Value::Array(vec![]).as_array().is_some());
        assert!(Value::Null.as_array().is_none());
    }
}
