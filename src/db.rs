use bimap::BiHashMap;

use redb::{Database, Error, ReadableDatabase, ReadableTable, TableDefinition};

pub const PAIRS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("pairs");
pub const AUTH_TABLE: TableDefinition<&str, &str> = TableDefinition::new("auth");
const DB_PATH: &str = "./filesync_rs_db.redb";

pub fn write(table: TableDefinition<&str, &str>, key: &str, value: &str) -> Result<(), Error> {
    let db = Database::create(DB_PATH)?;
    let write_txn = db.begin_write()?;
    {
        let mut table = write_txn.open_table(table)?;
        table.insert(key, value)?;
    }
    write_txn.commit()?;

    Ok(())
}

pub fn delete(table: TableDefinition<&str, &str>, key: &str) -> Result<(), Error> {
    let db = Database::create(DB_PATH)?;
    let write_txn = db.begin_write()?;
    {
        let mut table = write_txn.open_table(table)?;
        table.remove(key)?;
    }
    write_txn.commit()?;

    Ok(())
}

pub fn read_as_hashmap(table: TableDefinition<&str, &str>) -> Result<BiHashMap<String, String>, Error> {
    let db = Database::open(DB_PATH)?;
    let txn = db.begin_read()?;
    let table = txn.open_table(table)?;

    table
        .iter()?
        .map(|item| {
            let (key, value) = item?;
            Ok((key.value().to_string(), value.value().to_string()))
        })
        .collect()
}