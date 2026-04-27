// Copyright (c) 2015-2024 Georg Brandl.  Licensed under the Apache License,
// Version 2.0 <LICENSE-APACHE or http://www.apache.org/licenses/LICENSE-2.0>
// or the MIT license <LICENSE-MIT or http://opensource.org/licenses/MIT>, at
// your option. This file may not be copied, modified, or distributed except
// according to those terms.

//! QuickCheck Arbitrary instance for Value, and associated helpers.

use crate::value::{SmallTuple, SmallValue};
use crate::{HashableValue, Value};
use num_bigint::BigInt;
use quickcheck::{empty_shrinker, Arbitrary, Gen};
use rand::Rng;

const MAX_DEPTH: u32 = 1;

fn gen_value<G: Gen>(g: &mut G, depth: u32) -> Value {
    let upper = if depth > 0 { 14 } else { 7 };
    match g.gen_range(0, upper) {
        // leaves
        0 => Value::None,
        1 => Value::Bool(Arbitrary::arbitrary(g)),
        2 => Value::I64(Arbitrary::arbitrary(g)),
        3 => Value::Int(gen_bigint(g)),
        4 => Value::F64(Arbitrary::arbitrary(g)),
        5 => Value::Bytes(Arbitrary::arbitrary(g)),
        6 => Value::String(String::arbitrary(g).into()),
        // recursive variants
        7 => Value::List(gen_vec(g, depth - 1)),
        8 => Value::Tuple1(Arbitrary::arbitrary(g)),
        9 => Value::Tuple2(Arbitrary::arbitrary(g)),
        10 => Value::Tuple(gen_vec(g, depth - 1)),
        11 => Value::Set(gen_hvec(g, depth - 1).into_iter().collect()),
        12 => Value::FrozenSet(gen_hvec(g, depth - 1).into_iter().collect()),
        13 => {
            let kvec = gen_hvec(g, depth - 1);
            let vvec = gen_vec(g, depth - 1);
            Value::Dict(kvec.into_iter().zip(vvec).collect())
        }
        _ => unreachable!(),
    }
}

fn gen_bigint<G: Gen>(g: &mut G) -> BigInt {
    // We have to construct a value outside of i64 range, since other values
    // are unpickled as i64s instead of big ints.
    let offset = BigInt::from(2) * BigInt::from(if g.r#gen() { i64::MIN } else { i64::MAX });
    offset + BigInt::from(g.r#gen::<i64>())
}

fn gen_vec<G: Gen>(g: &mut G, depth: u32) -> Vec<Value> {
    let size = {
        let s = g.size();
        g.gen_range(0, s)
    };
    (0..size).map(|_| gen_value(g, depth)).collect()
}

fn gen_hvalue<G: Gen>(g: &mut G, depth: u32) -> HashableValue {
    let upper = if depth > 0 { 9 } else { 7 };
    match g.gen_range(0, upper) {
        // leaves
        0 => HashableValue::None,
        1 => HashableValue::Bool(Arbitrary::arbitrary(g)),
        2 => HashableValue::I64(Arbitrary::arbitrary(g)),
        3 => {
            // We have to construct a value outside of i64 range.
            let val: i64 = Arbitrary::arbitrary(g);
            let max = BigInt::from(i64::MAX);
            HashableValue::Int(BigInt::from(val) + BigInt::from(2) * max)
        }
        4 => HashableValue::F64(Arbitrary::arbitrary(g)),
        5 => HashableValue::Bytes(Arbitrary::arbitrary(g)),
        6 => HashableValue::String(String::arbitrary(g).into()),
        // recursive variants
        7 => HashableValue::Tuple(gen_hvec(g, depth - 1)),
        8 => HashableValue::FrozenSet(gen_hvec(g, depth - 1).into_iter().collect()),
        _ => unreachable!(),
    }
}

fn gen_hvec<G: Gen>(g: &mut G, depth: u32) -> Vec<HashableValue> {
    let size = {
        let s = g.size();
        g.gen_range(0, s)
    };
    (0..size).map(|_| gen_hvalue(g, depth)).collect()
}

impl Arbitrary for SmallValue {
    fn arbitrary<G: Gen>(g: &mut G) -> Self {
        match g.gen_range(0, 3) {
            // leaves
            0 => SmallValue::None,
            1 => SmallValue::Bool(Arbitrary::arbitrary(g)),
            2 => SmallValue::I32(Arbitrary::arbitrary(g)),
            //3 => SmallValue::F64(f64::arbitrary(g).try_into().unwrap_or_default()),
            _ => unreachable!(),
        }
    }
}

impl<const N: usize> Arbitrary for SmallTuple<N> {
    fn arbitrary<G: Gen>(g: &mut G) -> Self {
        Self(core::array::from_fn(|_| Arbitrary::arbitrary(g)))
    }
}

impl Arbitrary for Value {
    fn arbitrary<G: Gen>(g: &mut G) -> Value {
        gen_value(g, MAX_DEPTH)
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Value>> {
        match *self {
            Value::None => empty_shrinker(),
            Value::Bool(v) => Box::new(Arbitrary::shrink(&v).map(Value::Bool)),
            Value::I64(v) => Box::new(Arbitrary::shrink(&v).map(Value::I64)),
            Value::Int(_) => empty_shrinker(),
            Value::F64(v) => Box::new(Arbitrary::shrink(&v).map(Value::F64)),
            Value::Bytes(ref v) => Box::new(Arbitrary::shrink(v).map(Value::Bytes)),
            Value::String(ref v) => {
                Box::new(Arbitrary::shrink(&v.to_string()).map(Into::into).map(Value::String))
            }
            Value::List(ref v) => Box::new(Arbitrary::shrink(v).map(Value::List)),
            Value::Tuple1(ref v) => Box::new(Arbitrary::shrink(v).map(Value::Tuple1)),
            Value::Tuple2(ref v) => Box::new(Arbitrary::shrink(v).map(Value::Tuple2)),
            Value::Tuple3(ref v) => Box::new(Arbitrary::shrink(v).map(Value::Tuple3)),
            Value::Tuple(ref v) => Box::new(Arbitrary::shrink(v).map(Value::Tuple)),
            Value::Set(ref v) => Box::new(Arbitrary::shrink(v).map(Value::Set)),
            Value::FrozenSet(ref v) => Box::new(Arbitrary::shrink(v).map(Value::FrozenSet)),
            Value::Dict(ref v) => Box::new(Arbitrary::shrink(v).map(Value::Dict)),
        }
    }
}

impl Arbitrary for HashableValue {
    fn arbitrary<G: Gen>(g: &mut G) -> HashableValue {
        gen_hvalue(g, MAX_DEPTH)
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = HashableValue>> {
        match *self {
            HashableValue::None => empty_shrinker(),
            HashableValue::Bool(v) => Box::new(Arbitrary::shrink(&v).map(HashableValue::Bool)),
            HashableValue::I64(v) => Box::new(Arbitrary::shrink(&v).map(HashableValue::I64)),
            HashableValue::Int(_) => empty_shrinker(),
            HashableValue::F64(v) => Box::new(Arbitrary::shrink(&v).map(HashableValue::F64)),
            HashableValue::Bytes(ref v) => Box::new(Arbitrary::shrink(v).map(HashableValue::Bytes)),
            HashableValue::String(ref v) => {
                Box::new(Arbitrary::shrink(&v.to_string()).map(Into::into).map(HashableValue::String))
            }
            HashableValue::Tuple1(ref v) => Box::new(Arbitrary::shrink(v).map(HashableValue::Tuple1)),
            HashableValue::Tuple2(ref v) => Box::new(Arbitrary::shrink(v).map(HashableValue::Tuple2)),
            HashableValue::Tuple3(ref v) => Box::new(Arbitrary::shrink(v).map(HashableValue::Tuple3)),
            HashableValue::Tuple(ref v) => Box::new(Arbitrary::shrink(v).map(HashableValue::Tuple)),
            HashableValue::FrozenSet(ref v) => Box::new(Arbitrary::shrink(v).map(HashableValue::FrozenSet)),
        }
    }
}
