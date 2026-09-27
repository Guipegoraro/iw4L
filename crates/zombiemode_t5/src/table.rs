//! `TableLookUp` over the string tables a map's zones carry (`mp/zombiemode.csv`).

use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StringTable {
    rows: Vec<Vec<String>>,
}

impl StringTable {
    /// A table as the loader hands it over: one row per line, cells split on
    /// `,` (retail string tables hold no quoted commas).
    pub fn parse(csv: &str) -> Self {
        Self {
            rows: csv
                .lines()
                .map(|line| line.split(',').map(str::to_owned).collect())
                .collect(),
        }
    }

    /// `TableLookUp( table, search_column, value, return_column )`: the cell in
    /// `return_column` of the first row whose `search_column` is `value`, or
    /// `""` as retail returns for no match.
    pub fn lookup(&self, search_column: usize, value: &str, return_column: usize) -> &str {
        self.rows
            .iter()
            .find(|row| row.get(search_column).is_some_and(|cell| cell == value))
            .and_then(|row| row.get(return_column))
            .map_or("", String::as_str)
    }
}

/// Every string table the map loaded, by name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StringTables(BTreeMap<String, StringTable>);

impl StringTables {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn from_csv<'a>(tables: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        Self(
            tables
                .into_iter()
                .map(|(name, csv)| (name.to_owned(), StringTable::parse(csv)))
                .collect(),
        )
    }

    pub fn lookup(
        &self,
        table: &str,
        search_column: usize,
        value: &str,
        return_column: usize,
    ) -> &str {
        self.0
            .get(table)
            .map_or("", |t| t.lookup(search_column, value, return_column))
    }

    pub fn contains(&self, table: &str) -> bool {
        self.0.contains_key(table)
    }
}
