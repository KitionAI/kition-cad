//! What a reader reads from: a replica, or a transaction (the repair reads the merged state
//! through the session's `system` transaction, its own writes included).

use kition_kit::kdm::{ObjectId, Replica, Tx};
use serde_json::Value;

use crate::KdmResult;

pub(crate) trait Source {
    fn get(&self, id: &ObjectId, field: &str) -> KdmResult<Value>;
    fn list(&self, id: &ObjectId, field: &str) -> KdmResult<Vec<ObjectId>>;
    fn entries(&self, id: &ObjectId, field: &str) -> KdmResult<Vec<(String, Value)>>;
    fn exists(&self, id: &ObjectId) -> bool;
    fn type_of(&self, id: &ObjectId) -> Option<String>;
    /// A text field's characters; `None` where the source cannot read text (a transaction: the
    /// repair reads no cell text and writes none).
    fn text(&self, id: &ObjectId, field: &str) -> Option<String>;
}

impl Source for Replica {
    fn get(&self, id: &ObjectId, field: &str) -> KdmResult<Value> {
        Replica::get(self, id, field)
    }
    fn list(&self, id: &ObjectId, field: &str) -> KdmResult<Vec<ObjectId>> {
        Replica::list(self, id, field)
    }
    fn entries(&self, id: &ObjectId, field: &str) -> KdmResult<Vec<(String, Value)>> {
        Replica::map_entries(self, id, field)
    }
    fn exists(&self, id: &ObjectId) -> bool {
        Replica::exists(self, id)
    }
    fn type_of(&self, id: &ObjectId) -> Option<String> {
        Replica::object_type(self, id).ok()
    }
    fn text(&self, id: &ObjectId, field: &str) -> Option<String> {
        Replica::text(self, id, field).ok()
    }
}

impl Source for Tx<'_> {
    fn get(&self, id: &ObjectId, field: &str) -> KdmResult<Value> {
        Tx::get(self, id, field)
    }
    fn list(&self, id: &ObjectId, field: &str) -> KdmResult<Vec<ObjectId>> {
        Tx::list_items(self, id, field)
    }
    fn entries(&self, id: &ObjectId, field: &str) -> KdmResult<Vec<(String, Value)>> {
        Tx::map_entries(self, id, field)
    }
    fn exists(&self, id: &ObjectId) -> bool {
        Tx::exists(self, id)
    }
    fn type_of(&self, id: &ObjectId) -> Option<String> {
        Tx::object_type(self, id).ok()
    }
    fn text(&self, _id: &ObjectId, _field: &str) -> Option<String> {
        None
    }
}
