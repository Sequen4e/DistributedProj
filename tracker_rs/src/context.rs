use crate::models::{ FileManifest, PeerFileInfo };

use std::collections::HashMap;
use std::sync::Arc;
use dashmap::DashMap;
use tokio::sync::{Notify, RwLock};

pub struct Context {
    pub notify_save : Arc<Notify>,
    pub file_list: RwLock< Vec< Arc<FileManifest> > >,
    pub file_list_by_hash: RwLock< HashMap< String, Arc<FileManifest> > >,
    pub file_list_by_name: RwLock< HashMap< String, Vec< Arc<FileManifest> > > >,
    /// The key is the hash_set for a specific file
    pub seeding_peers: DashMap<String, Vec<PeerFileInfo> >
}

impl Context {
    pub fn new() -> Context {
        Context {
            notify_save: Arc::new(Notify::new()),
            file_list: RwLock::new(vec![]),
            file_list_by_hash: RwLock::new(HashMap::< String, Arc<FileManifest> >::new()),
            file_list_by_name: RwLock::new(HashMap::< String, Vec< Arc<FileManifest> > >::new()),
            seeding_peers: DashMap::<String, Vec<PeerFileInfo>>::new(),
        }
    }
}