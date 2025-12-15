use chrono::{Local, NaiveDateTime, TimeZone, Utc};
use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Display, Formatter};
use std::ops::Deref;

#[derive(Clone, Debug)]
pub struct DateTime<T: TimeZone>(chrono::DateTime<T>);

static FORMAT: &str = "%Y-%m-%d %H:%M:%S";

impl DateTime<Utc> {
    pub fn utc() -> Self {
        Self(Utc::now())
    }
}

impl DateTime<Local> {
    pub fn local() -> Self {
        Self(Local::now())
    }
}

impl DateTime<Utc> {
    pub fn parse(s: &str, fmt: &str) -> Result<Self, chrono::ParseError> {
        Ok(Self(NaiveDateTime::parse_from_str(s, fmt)?.and_utc()))
    }
}

impl DateTime<Local> {
    pub fn parse(s: &str, fmt: &str) -> Result<Self, chrono::ParseError> {
        Ok(Self(
            NaiveDateTime::parse_from_str(s, fmt)?
                .and_local_timezone(Local)
                .unwrap(),
        ))
    }
}

impl<T: TimeZone> DateTime<T>
where
    T::Offset: Display,
{
    pub fn format(&self, fmt: &str) -> String {
        self.0.format(fmt).to_string()
    }
}

impl<T: TimeZone> Deref for DateTime<T> {
    type Target = chrono::DateTime<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: TimeZone> Serialize for DateTime<T>
where
    T::Offset: Display,
{
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.format(FORMAT))
    }
}

impl<'de> Deserialize<'de> for DateTime<Utc> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct DateTimeVisitor;

        impl<'de> Visitor<'de> for DateTimeVisitor {
            type Value = DateTime<Utc>;

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                DateTime::<Utc>::parse(value, FORMAT).map_err(E::custom)
            }

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("struct 'DateTime<Utc>'")
            }
        }

        deserializer.deserialize_str(DateTimeVisitor)
    }
}

impl<'de> Deserialize<'de> for DateTime<Local> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct DateTimeVisitor;

        impl<'de> Visitor<'de> for DateTimeVisitor {
            type Value = DateTime<Local>;

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                DateTime::<Local>::parse(value, FORMAT).map_err(E::custom)
            }

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("struct 'DateTime<Local>'")
            }
        }

        deserializer.deserialize_str(DateTimeVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_de_datetime() {
        let now = DateTime::local();
        println!("{}", serde_json::to_string(&now).unwrap());
    }
}
