// Copyright (c) 2015-2024 Georg Brandl.  Licensed under the Apache License,
// Version 2.0 <LICENSE-APACHE or http://www.apache.org/licenses/LICENSE-2.0>
// or the MIT license <LICENSE-MIT or http://opensource.org/licenses/MIT>, at
// your option. This file may not be copied, modified, or distributed except
// according to those terms.

//! Python values, and serialization instances for them.

use compact_str::CompactString;
#[cfg(test)]
use either::Either;
use num_bigint::BigInt;
use num_traits::{Signed, ToPrimitive};
//use ordered_float::NotNan;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub use crate::value_impls::{from_value, to_value};

use crate::error::{Error, ErrorCode};

/// Optimization for small (1-3 element) tuples.  Only allows values up to i32, because that keeps
/// the SmallValue enum size down to 4 bytes; this means that, when used in 2- and 3-element
/// tuples, it doesn't blow the overall Value enum size past 32 bytes.  Values that contain an i64
/// will go in a normal heap-allocated Tuple variant.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SmallValue {
    None,
    Bool(bool),
    I32(i32),
    //F64(NotNan<f64>),
}

impl TryFrom<&Value> for SmallValue {
    type Error = ();
    #[inline]
    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        match value {
            Value::None => Ok(Self::None),
            Value::Bool(v) => Ok(Self::Bool(*v)),
            Value::I64(v) => (*v).try_into().map(Self::I32).map_err(|_| ()),
            //Value::F64(v) => (*v).try_into().map(Self::F64).map_err(|_| ()),
            _ => Err(()),
        }
    }
}

impl TryFrom<&HashableValue> for SmallValue {
    type Error = ();
    #[inline]
    fn try_from(value: &HashableValue) -> Result<Self, Self::Error> {
        match value {
            HashableValue::None => Ok(Self::None),
            HashableValue::Bool(v) => Ok(Self::Bool(*v)),
            HashableValue::I64(v) => (*v).try_into().map(Self::I32).map_err(|_| ()),
            //HashableValue::F64(v) => (*v).try_into().map(Self::F64).map_err(|_| ()),
            _ => Err(()),
        }
    }
}

impl Ord for SmallValue {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        use SmallValue::*;
        match *self {
            Self::None => match *other {
                None => Ordering::Equal,
                _ => Ordering::Less,
            },
            Bool(b) => match *other {
                None => Ordering::Greater,
                Bool(b2) => b.cmp(&b2),
                I32(i2) => (b as i32).cmp(&i2),
                //F64(f) => float_ord(b as i64 as f64, f.into_inner()),
            },
            I32(i) => match *other {
                None => Ordering::Greater,
                Bool(b) => i.cmp(&(b as i32)),
                I32(i2) => i.cmp(&i2),
                //F64(f) => float_ord(i as f64, f.into_inner()),
            },
            /*
            F64(f) => match *other {
                None => Ordering::Greater,
                Bool(b) => float_ord(f.into_inner(), b as i64 as f64),
                I32(i) => float_ord(f.into_inner(), i as f64),
                F64(f2) => f.cmp(&f2),
            },
            */
        }
    }
}

impl PartialOrd for SmallValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl From<SmallValue> for Value {
    #[inline]
    fn from(v: SmallValue) -> Self {
        match v {
            SmallValue::None => Self::None,
            SmallValue::Bool(v) => Self::Bool(v),
            SmallValue::I32(v) => Self::I64(v.into()),
            //SmallValue::F64(v) => Self::F64(v.into())
        }
    }
}

impl From<SmallValue> for HashableValue {
    #[inline]
    fn from(v: SmallValue) -> Self {
        match v {
            SmallValue::None => Self::None,
            SmallValue::Bool(v) => Self::Bool(v),
            SmallValue::I32(v) => Self::I64(v.into()),
            //SmallValue::F64(v) => Self::F64(v.into())
        }
    }
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub struct SmallTuple<const N: usize>(pub [SmallValue; N]);

impl<const N: usize> SmallTuple<N> {
    #[inline]
    pub(crate) fn as_values(&self) -> [Value; N] {
        self.0.map(Into::into)
    }

    #[inline]
    pub(crate) fn as_hashable_values(&self) -> [HashableValue; N] {
        self.0.map(Into::into)
    }

    #[cfg(test)]
    fn try_optimize(input: [Value; N]) -> Either<Self, [Value; N]> {
        let mut output = [SmallValue::None; N];
        for i in 0..N {
            output[i] = match SmallValue::try_from(&input[i]) {
                Ok(v) => v,
                Err(_) => return Either::Right(input),
            };
        }
        Either::Left(Self(output))
    }

    #[cfg(test)]
    fn try_optimize_hashable(input: [HashableValue; N]) -> Either<Self, [HashableValue; N]> {
        let mut output = [SmallValue::None; N];
        for i in 0..N {
            output[i] = match SmallValue::try_from(&input[i]) {
                Ok(v) => v,
                Err(_) => return Either::Right(input),
            };
        }
        Either::Left(Self(output))
    }
}

/// Represents all primitive builtin Python values that can be restored by
/// unpickling.
///
/// Note on integers: the distinction between the two types (short and long) is
/// very fuzzy in Python, and they can be used interchangeably.  In Python 3,
/// all integers are long integers, so all are pickled as such.  While decoding,
/// we simply put all integers that fit into an i64, and use `BigInt` for the
/// rest.
#[derive(Clone, Debug)]
pub enum Value {
    /// None
    None,
    /// Boolean
    Bool(bool),
    /// Short integer
    I64(i64),
    /// Long integer (unbounded length)
    Int(BigInt),
    /// Float
    F64(f64),
    /// Bytestring
    Bytes(Vec<u8>),
    /// Unicode string
    String(CompactString),
    /// List
    List(Vec<Value>),
    Tuple1(SmallTuple<1>),
    Tuple2(SmallTuple<2>),
    Tuple3(SmallTuple<3>),
    /// Tuple
    Tuple(Vec<Value>),
    /// Set
    Set(BTreeSet<HashableValue>),
    /// Frozen (immutable) set
    FrozenSet(BTreeSet<HashableValue>),
    /// Dictionary (map)
    Dict(BTreeMap<HashableValue, Value>),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        use self::Value::*;
        match *self {
            None => matches!(other, None),
            Bool(b) => match *other {
                Bool(b2) => b == b2,
                _ => false,
            },
            I64(i) => match *other {
                I64(i2) => i == i2,
                _ => false,
            },
            Int(ref bi) => match *other {
                Int(ref bi2) => bi == bi2,
                _ => false,
            },
            F64(f) => match *other {
                F64(f2) => f == f2,
                _ => false,
            },
            Bytes(ref bs) => match other {
                Bytes(bs2) => bs == bs2,
                _ => false,
            },
            String(ref s) => match *other {
                String(ref s2) => s == s2,
                _ => false,
            },
            List(ref l) => match *other {
                List(ref l2) => l == l2,
                _ => false,
            },
            Tuple1(ref t) => match *other {
                Tuple1(ref t2) => t == t2,
                Tuple(ref t2) => t.as_values().as_slice() == t2,
                _ => false,
            },
            Tuple2(ref t) => match *other {
                Tuple2(ref t2) => t == t2,
                Tuple(ref t2) => t.as_values().as_slice() == t2,
                _ => false,
            },
            Tuple3(ref t) => match *other {
                Tuple3(ref t2) => t == t2,
                Tuple(ref t2) => t.as_values().as_slice() == t2,
                _ => false,
            },
            Tuple(ref t) => match *other {
                Tuple(ref t2) => t == t2,
                Tuple1(ref t2) => t == t2.as_values().as_slice(),
                Tuple2(ref t2) => t == t2.as_values().as_slice(),
                Tuple3(ref t2) => t == t2.as_values().as_slice(),
                _ => false,
            },
            Set(ref s) => match *other {
                Set(ref s2) => s == s2,
                _ => false,
            },
            FrozenSet(ref s) => match *other {
                FrozenSet(ref s2) => s == s2,
                _ => false,
            },
            Dict(ref d) => match *other {
                Dict(ref d2) => d == d2,
                _ => false,
            },
        }
    }
}

impl Eq for Value {}

/// Represents all primitive builtin Python values that can be contained
/// in a "hashable" context (i.e., as dictionary keys and set elements).
///
/// In Rust, the type is *not* hashable, since we use B-tree maps and sets
/// instead of the hash variants.  To be able to put all Value instances
/// into these B-trees, we implement a consistent ordering between all
/// the possible types (see below).
#[derive(Clone, Debug)]
pub enum HashableValue {
    /// None
    None,
    /// Boolean
    Bool(bool),
    /// Short integer
    I64(i64),
    /// Long integer
    Int(BigInt),
    /// Float
    F64(f64),
    /// Bytestring
    Bytes(Vec<u8>),
    /// Unicode string
    String(CompactString),
    Tuple1(SmallTuple<1>),
    Tuple2(SmallTuple<2>),
    Tuple3(SmallTuple<3>),
    /// Tuple
    Tuple(Vec<HashableValue>),
    /// Frozen (immutable) set
    FrozenSet(BTreeSet<HashableValue>),
}

fn values_to_hashable(values: Vec<Value>) -> Result<Vec<HashableValue>, Error> {
    values.into_iter().map(Value::into_hashable).collect()
}

fn hashable_to_values(values: Vec<HashableValue>) -> Vec<Value> {
    values.into_iter().map(HashableValue::into_value).collect()
}

impl Value {
    /// Convert the value into a hashable version, if possible.  If not, return
    /// a ValueNotHashable error.
    pub fn into_hashable(self) -> Result<HashableValue, Error> {
        match self {
            Value::None => Ok(HashableValue::None),
            Value::Bool(b) => Ok(HashableValue::Bool(b)),
            Value::I64(i) => Ok(HashableValue::I64(i)),
            Value::Int(i) => Ok(HashableValue::Int(i)),
            Value::F64(f) => Ok(HashableValue::F64(f)),
            Value::Bytes(b) => Ok(HashableValue::Bytes(b)),
            Value::String(s) => Ok(HashableValue::String(s)),
            Value::FrozenSet(v) => Ok(HashableValue::FrozenSet(v)),
            Value::Tuple1(v) => Ok(HashableValue::Tuple1(v)),
            Value::Tuple2(v) => Ok(HashableValue::Tuple2(v)),
            Value::Tuple3(v) => Ok(HashableValue::Tuple3(v)),
            Value::Tuple(v) => values_to_hashable(v).map(HashableValue::Tuple),
            _ => Err(Error::Syntax(ErrorCode::ValueNotHashable)),
        }
    }

    #[cfg(test)]
    pub fn construct_tuple(values: &[Value]) -> Self {
        match values {
            [item1] => match SmallTuple::try_optimize([item1.clone()]) {
                Either::Left(v) => Self::Tuple1(v),
                Either::Right(v) => Self::Tuple(v.into()),
            },
            [item1, item2] => match SmallTuple::try_optimize([item1.clone(), item2.clone()]) {
                Either::Left(v) => Self::Tuple2(v),
                Either::Right(v) => Self::Tuple(v.into()),
            },
            [item1, item2, item3] => {
                match SmallTuple::try_optimize([item1.clone(), item2.clone(), item3.clone()]) {
                    Either::Left(v) => Self::Tuple3(v),
                    Either::Right(v) => Self::Tuple(v.into()),
                }
            }
            _ => Self::Tuple(values.to_owned().into()),
        }
    }
}

impl HashableValue {
    /// Convert the value into its non-hashable version.  This always works.
    pub fn into_value(self) -> Value {
        match self {
            HashableValue::None => Value::None,
            HashableValue::Bool(b) => Value::Bool(b),
            HashableValue::I64(i) => Value::I64(i),
            HashableValue::Int(i) => Value::Int(i),
            HashableValue::F64(f) => Value::F64(f),
            HashableValue::Bytes(b) => Value::Bytes(b),
            HashableValue::String(s) => Value::String(s),
            HashableValue::FrozenSet(v) => Value::FrozenSet(v),
            HashableValue::Tuple1(v) => Value::Tuple1(v),
            HashableValue::Tuple2(v) => Value::Tuple2(v),
            HashableValue::Tuple3(v) => Value::Tuple3(v),
            HashableValue::Tuple(v) => Value::Tuple(hashable_to_values(v)),
        }
    }

    #[cfg(test)]
    pub fn construct_tuple(values: &[HashableValue]) -> Self {
        match values {
            [item1] => match SmallTuple::try_optimize_hashable([item1.clone()]) {
                Either::Left(v) => Self::Tuple1(v),
                Either::Right(v) => Self::Tuple(v.into()),
            },
            [item1, item2] => match SmallTuple::try_optimize_hashable([item1.clone(), item2.clone()]) {
                Either::Left(v) => Self::Tuple2(v),
                Either::Right(v) => Self::Tuple(v.into()),
            },
            [item1, item2, item3] => {
                match SmallTuple::try_optimize_hashable([item1.clone(), item2.clone(), item3.clone()]) {
                    Either::Left(v) => Self::Tuple3(v),
                    Either::Right(v) => Self::Tuple(v.into()),
                }
            }
            _ => Self::Tuple(values.to_owned().into()),
        }
    }
}

fn write_elements<'a, I, T>(
    f: &mut fmt::Formatter, it: I, prefix: &'static str, suffix: &'static str, len: usize, always_comma: bool,
) -> fmt::Result
where
    I: Iterator<Item = &'a T>,
    T: fmt::Display + 'a,
{
    f.write_str(prefix)?;
    for (i, item) in it.enumerate() {
        if i < len - 1 || always_comma {
            write!(f, "{}, ", item)?;
        } else {
            write!(f, "{}", item)?;
        }
    }
    f.write_str(suffix)
}

impl fmt::Display for SmallValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            SmallValue::None => write!(f, "None"),
            SmallValue::Bool(b) => write!(f, "{}", if b { "True" } else { "False" }),
            SmallValue::I32(i) => write!(f, "{}", i),
            //SmallValue::F64(v) => write!(f, "{}", v),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Value::None => write!(f, "None"),
            Value::Bool(b) => write!(f, "{}", if b { "True" } else { "False" }),
            Value::I64(i) => write!(f, "{}", i),
            Value::Int(ref i) => write!(f, "{}", i),
            Value::F64(v) => write!(f, "{}", v),
            Value::Bytes(ref b) => write!(f, "b{:?}", b), //
            Value::String(ref s) => write!(f, "{:?}", s),
            Value::List(ref v) => write_elements(f, v.iter(), "[", "]", v.len(), false),
            Value::Tuple1(ref v) => write_elements(f, v.0.iter(), "(", ")", v.0.len(), true),
            Value::Tuple2(ref v) => write_elements(f, v.0.iter(), "(", ")", v.0.len(), false),
            Value::Tuple3(ref v) => write_elements(f, v.0.iter(), "(", ")", v.0.len(), false),
            Value::Tuple(ref v) => write_elements(f, v.iter(), "(", ")", v.len(), v.len() == 1),
            Value::FrozenSet(ref v) => write_elements(f, v.iter(), "frozenset([", "])", v.len(), false),
            Value::Set(ref v) => {
                if v.is_empty() {
                    write!(f, "set()")
                } else {
                    write_elements(f, v.iter(), "{", "}", v.len(), false)
                }
            }
            Value::Dict(ref v) => {
                write!(f, "{{")?;
                for (i, (key, value)) in v.iter().enumerate() {
                    if i < v.len() - 1 {
                        write!(f, "{}: {}, ", key, value)?;
                    } else {
                        write!(f, "{}: {}", key, value)?;
                    }
                }
                write!(f, "}}")
            }
        }
    }
}

impl fmt::Display for HashableValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            HashableValue::None => write!(f, "None"),
            HashableValue::Bool(b) => write!(f, "{}", if b { "True" } else { "False" }),
            HashableValue::I64(i) => write!(f, "{}", i),
            HashableValue::Int(ref i) => write!(f, "{}", i),
            HashableValue::F64(v) => write!(f, "{}", v),
            HashableValue::Bytes(ref b) => write!(f, "b{:?}", b), //
            HashableValue::String(ref s) => write!(f, "{:?}", s),
            HashableValue::Tuple1(ref v) => write_elements(f, v.0.iter(), "(", ")", v.0.len(), true),
            HashableValue::Tuple2(ref v) => write_elements(f, v.0.iter(), "(", ")", v.0.len(), false),
            HashableValue::Tuple3(ref v) => write_elements(f, v.0.iter(), "(", ")", v.0.len(), false),
            HashableValue::Tuple(ref v) => write_elements(f, v.iter(), "(", ")", v.len(), v.len() == 1),
            HashableValue::FrozenSet(ref v) => {
                write_elements(f, v.iter(), "frozenset([", "])", v.len(), false)
            }
        }
    }
}

impl PartialEq for HashableValue {
    fn eq(&self, other: &HashableValue) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for HashableValue {}

impl PartialOrd for HashableValue {
    fn partial_cmp(&self, other: &HashableValue) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Implement a (more or less) consistent ordering for `HashableValue`s
/// so that they can be added to dictionaries and sets.
///
/// Also, like in Python, numeric values with the same value (integral or not)
/// must compare equal.
///
/// For other types, we define an ordering between all types A and B so that all
/// objects of type A are always lesser than objects of type B.  This is done
/// similar to Python 2's ordering of different types.
impl Ord for HashableValue {
    fn cmp(&self, other: &HashableValue) -> Ordering {
        use self::HashableValue::*;
        match *self {
            None => match *other {
                None => Ordering::Equal,
                _ => Ordering::Less,
            },
            Bool(b) => match *other {
                None => Ordering::Greater,
                Bool(b2) => b.cmp(&b2),
                I64(i2) => (b as i64).cmp(&i2),
                Int(ref bi) => BigInt::from(b as i64).cmp(bi),
                F64(f) => float_ord(b as i64 as f64, f),
                _ => Ordering::Less,
            },
            I64(i) => match *other {
                None => Ordering::Greater,
                Bool(b) => i.cmp(&(b as i64)),
                I64(i2) => i.cmp(&i2),
                Int(ref bi) => BigInt::from(i).cmp(bi),
                F64(f) => float_ord(i as f64, f),
                _ => Ordering::Less,
            },
            Int(ref bi) => match *other {
                None => Ordering::Greater,
                Bool(b) => bi.cmp(&BigInt::from(b as i64)),
                I64(i) => bi.cmp(&BigInt::from(i)),
                Int(ref bi2) => bi.cmp(bi2),
                F64(f) => float_bigint_ord(bi, f),
                _ => Ordering::Less,
            },
            F64(f) => match *other {
                None => Ordering::Greater,
                Bool(b) => float_ord(f, b as i64 as f64),
                I64(i) => float_ord(f, i as f64),
                Int(ref bi) => BigInt::from(f as i64).cmp(bi),
                F64(f2) => float_ord(f, f2),
                _ => Ordering::Less,
            },
            Bytes(ref bs) => match *other {
                String(_) | FrozenSet(_) | Tuple(_) => Ordering::Less,
                Bytes(ref bs2) => bs.cmp(bs2),
                _ => Ordering::Greater,
            },
            String(ref s) => match *other {
                FrozenSet(_) | Tuple(_) => Ordering::Less,
                String(ref s2) => s.cmp(s2),
                _ => Ordering::Greater,
            },
            FrozenSet(ref s) => match *other {
                Tuple1(_) | Tuple2(_) | Tuple3(_) | Tuple(_) => Ordering::Less,
                FrozenSet(ref s2) => s.cmp(s2),
                _ => Ordering::Greater,
            },
            Tuple1(ref t) => match *other {
                Tuple1(ref t2) => t.cmp(t2),
                Tuple(ref t2) => t.as_hashable_values().as_slice().cmp(t2),
                _ => Ordering::Greater,
            },
            Tuple2(ref t) => match *other {
                Tuple2(ref t2) => t.cmp(t2),
                Tuple(ref t2) => t.as_hashable_values().as_slice().cmp(t2),
                _ => Ordering::Greater,
            },
            Tuple3(ref t) => match *other {
                Tuple3(ref t2) => t.cmp(t2),
                Tuple(ref t2) => t.as_hashable_values().as_slice().cmp(t2),
                _ => Ordering::Greater,
            },
            Tuple(ref t) => match *other {
                Tuple(ref t2) => t.cmp(t2),
                Tuple1(ref t2) => t.as_slice().cmp(t2.as_hashable_values().as_slice()),
                Tuple2(ref t2) => t.as_slice().cmp(t2.as_hashable_values().as_slice()),
                Tuple3(ref t2) => t.as_slice().cmp(t2.as_hashable_values().as_slice()),
                _ => Ordering::Greater,
            },
        }
    }
}

/// A "reasonable" total ordering for floats.
fn float_ord(f: f64, g: f64) -> Ordering {
    match f.partial_cmp(&g) {
        Some(o) => o,
        None => Ordering::Less,
    }
}

/// Ordering between floats and big integers.
fn float_bigint_ord(bi: &BigInt, g: f64) -> Ordering {
    match bi.to_f64() {
        Some(f) => float_ord(f, g),
        None => {
            if bi.is_positive() {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        }
    }
}
