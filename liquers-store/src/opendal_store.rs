
use liquers_core::{error::Error, metadata::{Metadata, MetadataRecord}, query::Key, store::Store};
use opendal::{BlockingOperator, Operator, Buffer};
use bytes::Buf;

pub struct OpenDALStore {
    op: BlockingOperator,
    prefix: Key,
}

impl OpenDALStore {
    const METADATA: &'static str = ".__metadata__";

    pub fn new(op: BlockingOperator, prefix: Key) -> Self {
        OpenDALStore { op, prefix }
    }

    pub fn key_to_path(&self, key: &Key) -> String {
        key.encode()
    }

    pub fn key_to_path_metadata(&self, key: &Key) -> String {
        format!("{}{}", key.encode(), Self::METADATA)
    }
    fn map_read_error<T>(&self, key:&Key, res:opendal::Result<T>)->Result<T, liquers_core::error::Error> {
        res.map_err(|e| liquers_core::error::Error::key_read_error(key, &self.store_name(), &format!("{e} (OpenDAL Read Error)")))
    }
    fn map_write_error<T>(&self, key:&Key, res:opendal::Result<T>)->Result<T, liquers_core::error::Error> {
        res.map_err(|e| liquers_core::error::Error::key_write_error(key, &self.store_name(), &format!("{e} (OpenDAL Write Error)")))
    }

}

impl Store for OpenDALStore {
    fn store_name(&self) -> String {
        "OpenDALStore".to_string()
    }

    fn key_prefix(&self) -> Key {
        self.prefix.clone()
    }
    
    fn default_metadata(&self, _key: &Key, _is_dir: bool) -> MetadataRecord {
        MetadataRecord::new()
    }
    
    fn finalize_metadata(
        &self,
        metadata: Metadata,
        _key: &Key,
        _data: &[u8],
        _update: bool,
    ) -> Metadata {
        metadata
    }
    
    fn finalize_metadata_empty(
        &self,
        metadata: Metadata,
        _key: &Key,
        _is_dir: bool,
        _update: bool,
    ) -> Metadata {
        metadata
    }
    
    fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), liquers_core::error::Error> {
        Ok((self.get_bytes(key)?, self.get_metadata(key)?))
    }
    
    fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, liquers_core::error::Error> {
        let path = self.key_to_path(key);
        let buf = self.map_read_error(key, self.op.read(&path))?;
        Ok(buf.to_vec())
    }
    
    fn get_metadata(&self, key: &Key) -> Result<Metadata, liquers_core::error::Error> {
        let path = self.key_to_path_metadata(key);
        if self.map_read_error(key, self.op.exists(&path))? {
            let buffer = self.map_read_error(key, self.op.read(&path))?;
            if let Ok(metadata) = serde_json::from_reader(buffer.reader()) {
                return Ok(Metadata::MetadataRecord(metadata));
            }
            let buffer = self.map_read_error(key, self.op.read(&path))?;
            if let Ok(metadata) = serde_json::from_reader(buffer.reader()) {
                return Ok(Metadata::LegacyMetadata(metadata));
            }
            Err(Error::key_read_error(
                key,
                &self.store_name(),
                "Metadata parsing error",
            ))
        } else {
            Err(Error::key_not_found(key))
        }
    }
    
    fn set(&mut self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), liquers_core::error::Error> {
        //TODO: create_dir
        let path = self.key_to_path(key);
        let buffer = Buffer::from_iter(data.iter().copied());
        self.map_write_error(key, self.op.write(&path, buffer))?;
        self.set_metadata(key, metadata)?;
        Ok(())
    }
    
    fn set_metadata(&mut self, key: &Key, metadata: &Metadata) -> Result<(), liquers_core::error::Error> {
        //TODO: create_dir
        let path = self.key_to_path_metadata(key);
        let mut file = self.map_write_error(key, self.op.writer(&path))?.into_std_write();
        match metadata {
            Metadata::MetadataRecord(metadata) => serde_json::to_writer_pretty(file, metadata)
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?,
            Metadata::LegacyMetadata(metadata) => serde_json::to_writer_pretty(file, metadata)
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?,
        };
        Ok(())
    }
    
    fn remove(&mut self, key: &Key) -> Result<(), liquers_core::error::Error> {
        let path = self.key_to_path(key);
        if self.map_read_error(key, self.op.exists(&path))? {
            self.map_write_error(key, self.op.delete(&path))?;
        }
        let matadata_path = self.key_to_path_metadata(key);
        if self.map_read_error(key, self.op.exists(&matadata_path))? {
            self.map_write_error(key, self.op.delete(&matadata_path))?;
        }
        Ok(())
    }
    
    fn removedir(&mut self, key: &Key) -> Result<(), liquers_core::error::Error> {
        todo!()
    }
    
    fn contains(&self, _key: &Key) -> Result<bool, liquers_core::error::Error> {
        todo!()
    }
    
    fn is_dir(&self, _key: &Key) -> Result<bool, liquers_core::error::Error> {
        todo!()
    }
    
    fn keys(&self) -> Result<Vec<Key>, liquers_core::error::Error> {
        let mut keys = self.listdir_keys_deep(&self.key_prefix())?;
        keys.push(self.key_prefix().to_owned());
        Ok(keys)
    }
    
    fn listdir(&self, _key: &Key) -> Result<Vec<String>, liquers_core::error::Error> {
        todo!()
    }
    
    fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, liquers_core::error::Error> {
        let names = self.listdir(key)?;
        Ok(names.iter().map(|x| key.join(x)).collect())
    }
    
    fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, liquers_core::error::Error> {
        let keys = self.listdir_keys(key)?;
        let mut keys_deep = keys.clone();
        for sub_key in keys {
            if self.is_dir(&key)? {
                let sub = self.listdir_keys_deep(&sub_key)?;
                keys_deep.extend(sub.into_iter());
            }
        }
        Ok(keys_deep)
    }
    
    fn makedir(&self, key: &Key) -> Result<(), liquers_core::error::Error> {
        todo!()
    }
    
    fn is_supported(&self, key: &Key) -> bool {
        key.has_key_prefix(&self.prefix)
            && (!key
                .filename()
                .is_some_and(|file_name| file_name.name.ends_with(Self::METADATA)))
    }


}