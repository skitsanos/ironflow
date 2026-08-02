use std::io::{self, Write};

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SizeBound {
    Within,
    Exceeded,
}

/// Count compact JSON bytes without retaining them, stopping as soon as the
/// configured boundary is crossed.
pub(super) fn serialized_size_up_to<T>(value: &T, limit: usize) -> SizeBound
where
    T: Serialize + ?Sized,
{
    let mut writer = BoundedCountWriter::new(limit);
    let result = serde_json::to_writer(&mut writer, value);
    if writer.exceeded || result.is_err() {
        // JSON Values cannot otherwise fail to serialize. Treat an unexpected
        // serializer failure as over-limit rather than publishing unchecked.
        return SizeBound::Exceeded;
    }
    SizeBound::Within
}

struct BoundedCountWriter {
    written: usize,
    limit: usize,
    exceeded: bool,
}

impl BoundedCountWriter {
    fn new(limit: usize) -> Self {
        Self {
            written: 0,
            limit,
            exceeded: false,
        }
    }
}

impl Write for BoundedCountWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(next) = self.written.checked_add(bytes.len()) else {
            self.exceeded = true;
            return Err(boundary_error());
        };
        if next > self.limit {
            self.exceeded = true;
            return Err(boundary_error());
        }
        self.written = next;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn boundary_error() -> io::Error {
    io::Error::other("serialized value exceeded its byte boundary")
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use serde::ser::{SerializeSeq, Serializer};

    use super::*;

    struct CountedItems<'a> {
        visits: &'a Cell<usize>,
        total: usize,
    }

    impl Serialize for CountedItems<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut sequence = serializer.serialize_seq(Some(self.total))?;
            for _ in 0..self.total {
                self.visits.set(self.visits.get() + 1);
                sequence.serialize_element("0123456789")?;
            }
            sequence.end()
        }
    }

    #[test]
    fn counting_stops_after_crossing_the_limit() {
        let visits = Cell::new(0);
        let value = CountedItems {
            visits: &visits,
            total: 10_000,
        };

        assert_eq!(serialized_size_up_to(&value, 64), SizeBound::Exceeded);
        assert!(
            visits.get() < 10,
            "serializer traversed {} items",
            visits.get()
        );
    }

    #[test]
    fn exact_boundary_is_accepted() {
        assert_eq!(serialized_size_up_to("abc", 5), SizeBound::Within);
        assert_eq!(serialized_size_up_to("abc", 4), SizeBound::Exceeded);
    }
}
