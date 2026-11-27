//! Stream wrapper arrays into a bounded channel; only a single account needs to fit in memory.
use crate::Error;
use serde::{
    de::{self, value::MapAccessDeserializer, DeserializeSeed, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer,
};
use serde_json::{Map, Value};
use std::{
    fmt,
    io::{BufRead, BufReader, Read},
    path::Path,
};
use tokio::sync::mpsc::Sender;
struct Records<'a>(&'a Sender<Value>);
fn deliver<E: de::Error>(tx: &Sender<Value>, v: Value) -> Result<(), E> {
    tx.blocking_send(v).map_err(|_| E::custom("导入已停止"))
}
fn sequence<'de, A: SeqAccess<'de>>(tx: &Sender<Value>, mut seq: A) -> Result<(), A::Error> {
    while let Some(v) = seq.next_element::<Value>()? {
        deliver(tx, v)?;
    }
    Ok(())
}
impl<'de> Visitor<'de> for Records<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("账号、账号数组或包装对象")
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<(), E> {
        deliver(self.0, Value::String(v.into()))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<(), A::Error> {
        sequence(self.0, seq)
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut single = Map::new();
        let mut batch = false;
        while let Some(key) = map.next_key::<String>()? {
            if matches!(key.as_str(), "accounts" | "items" | "credentials") && !batch {
                match map.next_value_seed(Wrapped(self.0))? {
                    Some(v) => {
                        single.insert(key, v);
                    }
                    None => batch = true,
                }
            } else if batch {
                map.next_value::<de::IgnoredAny>()?;
            } else {
                single.insert(key, map.next_value()?);
            }
        }
        if !batch {
            deliver(self.0, Value::Object(single))?;
        }
        Ok(())
    }
}
struct Wrapped<'a>(&'a Sender<Value>);
impl<'de> DeserializeSeed<'de> for Wrapped<'_> {
    type Value = Option<Value>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Wrapped<'_> {
    type Value = Option<Value>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("凭证对象或账号数组")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
        sequence(self.0, seq)?;
        Ok(None)
    }
    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        Value::deserialize(MapAccessDeserializer::new(map)).map(Some)
    }
}
pub(crate) fn parse(path: &Path, tx: Sender<Value>) -> Result<(), Error> {
    let mut reader = BufReader::new(std::fs::File::open(path)?);
    let byte = loop {
        let b = reader.fill_buf()?;
        if b.is_empty() {
            return Err(Error::Invalid("空文件"));
        }
        if let Some(i) = b.iter().position(|x| !x.is_ascii_whitespace()) {
            let v = b[i];
            reader.consume(i);
            break v;
        }
        let len = b.len();
        reader.consume(len);
    };
    if matches!(byte, b'{' | b'[' | b'"') {
        let mut de = serde_json::Deserializer::from_reader(reader);
        (&mut de).deserialize_any(Records(&tx))?;
        de.end()?;
    } else {
        let mut b = String::new();
        reader.take(16385).read_to_string(&mut b)?;
        let s = b.trim();
        if b.len() > 16384 || s.is_empty() || s.chars().any(char::is_whitespace) {
            return Err(Error::Invalid("裸凭证无效"));
        }
        tx.blocking_send(Value::String(s.into()))
            .map_err(|_| Error::Invalid("导入已停止"))?;
    }
    Ok(())
}
