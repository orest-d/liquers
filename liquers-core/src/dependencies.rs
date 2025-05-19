
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// A version is a 128-bit integer that is used to identify the version of an asset.
pub struct Version(u128);

use std::collections::HashSet;
use std::fmt::{Debug, Display};
use std::os::linux::raw::stat;

use blake3::Hash;
use nom::Err;

use crate::error::Error;
use crate::query::{Key, Query};
use crate::metadata::Status;

impl Version {
    pub fn new(version: u128) -> Self {
        Version(version)
    }

    /// Creates a new version from bytes.
    /// Implemented as a hash the bytes using BLAKE3 and convert the first 16 bytes to i128.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let hash_obj = blake3::hash(bytes);
        let hash = hash_obj.as_bytes();
        let version = u128::from_be_bytes([
            hash[0], hash[1], hash[2], hash[3], hash[4], hash[5], hash[6], hash[7],
            hash[8], hash[9], hash[10], hash[11], hash[12], hash[13], hash[14], hash[15],
        ]);
        
        Version(version)
    }

    /// Creates a new version as a timestamp.
    pub fn from_time_now() -> Self {
        let now = std::time::SystemTime::now();
        let duration = now.duration_since(std::time::UNIX_EPOCH).unwrap();
        Version(duration.as_nanos())
    }
}


trait Dependency: Display + Clone + PartialEq + Eq + std::hash::Hash + Debug {
    /// Only base dependencies should be used for tracking.
    /// Base dependency is more permanent - it is typically stored in a store.
    /// Besides files, it could be recipes or commands.
    /// Results of queries in general are not base dependencies, since they are nonsidered permanent.
    /// Results of queries may be stored in cache, where they may be invalidated.
    /// However, cache may be emptied any time, which would invalidate the dependencies that would depend on such an object.
    fn is_base_dependency(&self) -> bool;
}
/// A dependency tracks a dependency of an asset.
/// A dependency can refer to another asset via a key or query, command or special dependencies (e.g. key existence).
/// Dependency must be serializable and deserializable in form of a string in order to be stored in a database.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StringDependency(String);

impl std::fmt::Display for StringDependency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl StringDependency {
    pub fn new(name: &str) -> Self {
        StringDependency(name.to_string())
    }

    pub fn from_string(name: &str) -> Self {
        StringDependency(name.to_string())
    }

    pub fn from_query(query:&Query) -> Self {
        StringDependency(format!("query:{}",query.encode()))
    }

    pub fn from_key(key:&Key) -> Self {
        StringDependency(format!("key:{}",key.encode()))
    }

}

impl Dependency for StringDependency {
    fn is_base_dependency(&self) -> bool {
        self.0.starts_with("key:")
    }
}

/// A dependency record is a record stating a version and status of a single dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyRecord<V: Clone + PartialEq + Eq + Debug, D: Dependency> {
    /// The dependency characterized by version and status.
    pub dependency: D,
    /// The version of the dependency.
    pub version: V,
    /// The status of the dependency.
    pub status: Status,
    /// User or system specified comment - explains the purpose or source of the dependency
    pub comment:String
}

impl<V: Clone + PartialEq + Eq + Debug, D: Dependency> DependencyRecord<V, D> {
    pub fn new(dependency: D, version: V, status: Status) -> Self {
        DependencyRecord {
            dependency,
            version,
            status,
            comment:"".to_string()
        }
    }
    pub fn with_comment(&mut self, comment:String)->&mut Self{
        self.comment = comment;
        self
    }
}

/// A dependency list is a list of all direct dependencies of a dependant
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyList<V: Clone + PartialEq + Eq + Debug, D: Dependency> {
    pub dependant: D,
    pub version:V,
    pub status:Status,
    pub dependencies:Vec<DependencyRecord<V,D>>,
}

impl<V: Clone + PartialEq + Eq + Debug, D: Dependency> DependencyList<V, D> {
    pub fn new(dependant: D, version:V, status:Status, dependencies:Vec<DependencyRecord<V, D>>) -> Self {
        DependencyList {
            dependant,
            version,
            status,
            dependencies
        }
    }
}

/// A dependency manager is a trait that manages the dependencies of an asset.
/// It is used to track the dependencies of an asset.
/// It supports following operations:
/// - set (and get) a version and status of a dependency
/// - specify all dependencies of an asset
/// 
/// Changing the version or status of a dependency will trigger changes of status of dependents.
/// Result of such a change is thus a list of impacted dependents.
pub trait DependencyManager {

    type ManagedVersion: Clone + PartialEq + Eq;
    type ManagedDependency: Dependency;

    //fn set_status(&mut self, dependency:&Self::Dependency, version:Self::Version, status: Status) -> Result<Vec<Self::Dependency>, Error>;
    //fn get_status(&self, dependency:&Self::Dependency) -> Result<(Version, Status), Error>;
    //fn add_dependency(&mut self, dependency:&Self::Dependency, version: Self::Version, status: Status, critical:bool) -> Result<Vec<Self::Dependency>, Error>;
}

pub struct DependencyManagerImpl<V: Clone + PartialEq + Eq + Debug, D: Dependency> {
    status: std::collections::HashMap<D, (Status,V)>,
    dependencies: std::collections::HashMap<D, Vec<DependencyRecord<V, D>>>,
    dependents: std::collections::HashMap<D, HashSet<D>>,
}

impl<V: Clone + PartialEq + Eq + Debug, D: Dependency> DependencyManagerImpl<V, D> {
    pub fn new() -> Self {
        DependencyManagerImpl {
            status: std::collections::HashMap::new(),
            dependencies: std::collections::HashMap::new(),
            dependents: std::collections::HashMap::new(),
        }
    }

    /// Find a set of all dependencies of a dependency.
    /// This is a recursive function that will find all dependencies of a dependency.
    /// 
    /// ```
    /// use std::collections::HashSet;
    /// use liquers_core::dependencies::{DependencyManagerImpl, StringDependency};
    /// use liquers_core::metadata::Status;
    /// 
    /// let mut dm = DependencyManagerImpl::<i32, StringDependency>::new();
    /// let x = StringDependency::new("x");
    /// let y = StringDependency::new("y");
    /// let z = StringDependency::new("z");
    /// dm.add_new(&x, 1, Status::Ready, &mut HashSet::new()).unwrap();
    /// dm.add_new(&y, 2, Status::Ready, &mut HashSet::new()).unwrap();
    /// dm.add_new(&z, 3, Status::Ready, &mut HashSet::new()).unwrap();
    /// assert_eq!(dm.all_dependents(&y).len(), 0);
    /// 
    /// dm.add_raw_dependency(&x, &y, &mut HashSet::new()).unwrap();
    /// assert_eq!(dm.all_dependents(&y).len(), 1);
    ///
    /// dm.add_raw_dependency(&y, &z, &mut HashSet::new()).unwrap();
    /// assert_eq!(dm.all_dependents(&z).len(), 2);
    /// assert_eq!(dm.all_dependents(&z).contains(&y), true);
    /// assert_eq!(dm.all_dependents(&z).contains(&x), true);
    /// ```
    pub fn all_dependents(&self, something:&D) -> HashSet<D> {
        let mut result = HashSet::new();
        if let Some(dependents) = self.dependents.get(something) {
            for dependent in dependents {
                result.insert(dependent.clone());
                result.extend(self.all_dependents(dependent));
            }
        }
        result
    }

    pub fn is_base_dependency(&self, dependency:&D) -> bool {
        dependency.is_base_dependency()
    }

    pub fn base_dependencies(&self, dependency:&D) -> HashSet<D> {
        let mut result = HashSet::new();
        if self.is_base_dependency(dependency) {
            result.insert(dependency.clone());
            return result;
        }
        if let Some(d) = self.dependencies.get(dependency) {
            for record in d 
            {
                if record.dependency.is_base_dependency() {
                    result.insert(record.dependency.clone());
                }
                else{
                    result.extend(self.base_dependencies(&record.dependency));
                }
            }
        }
        result
    }

    /// Check is a dependent depends on a dependency.
    /// This is a recursive function that will check if a dependent depends on a dependency.
    /// This is used to prevent creation of circular dependencies.
    pub fn depends(&self, dependent:&D, dependency:&D) -> bool {
        if let Some(dependents) = self.dependents.get(dependency) {
            if dependents.contains(dependent) {
                return true;
            }
            for dep in dependents {
                if self.depends(dependent, dep) {
                    return true;
                }
            }
        }
        false
    }

    /// Expire a dependency.
    /// This will remove the dependency and its dependent and mark them as expired.
    /// Impacted dependencies are added to a set of impacted dependencies.
    pub fn expire(&mut self, dependency:&D, impacted: &mut HashSet<D>) -> Result<(), Error> {
        if let Some(status) = self.status.get_mut(dependency) {
            if status.0.can_have_tracked_dependencies(){
                status.0 = Status::Expired;
            }
        }
        for d in self.all_dependents(dependency){
            self.status.entry((&d).clone()).and_modify(
                |x|{
                    if x.0.has_data(){
                        x.0 = Status::Expired;
                    }
                }
            );
            self.dependents.remove(&d);
            impacted.insert(d);
        }
        impacted.insert(dependency.clone());
        self.dependents.remove(dependency);
        Ok(())
    }

    pub fn add_new(&mut self, something:&D, version: V, status: Status, impacted: &mut HashSet<D>) -> Result<(), Error> {
        self.expire(something, impacted)?;
        if self.status.get(something).is_none() {
            self.status.insert(something.clone(), (status, version));
        }
        Ok(())
    }
    /// Create a link between "something" and its dependency.
    /// The dependency must have a known status and version.
    /// The dependency should be a base dependency.
    pub fn add_raw_dependency(&mut self, something:&D, dependency:&D, impacted: &mut HashSet<D>) -> Result<(), Error> {
        if self.dependencies.get(something).is_none() {
            self.dependencies.insert(something.clone(), vec![]);
        }        
        if self.dependents.get(dependency).is_none() {
            self.dependents.insert(dependency.clone(), HashSet::new());
        }
        self.dependents.get_mut(dependency).unwrap().insert(something.clone());
        if self.dependencies.get_mut(something).unwrap().iter().all(|record| record.dependency != *dependency) {
            if let Some((status, version)) = self.status.get(dependency){
                let record = DependencyRecord::new(dependency.clone(), version.clone(), status.clone());
                self.dependencies.get_mut(something).unwrap().push(record);
                impacted.insert(something.clone());
                if !status.has_data() {
                    self.expire(something, impacted);
                }                
            }
            else{
                return Err(Error::general_error(format!("Dependency {dependency} not found")));
            }
        }
        Ok(())
    }

    pub fn print(&self){
        println!("Status:");
        for (key, value) in &self.status {
            println!("  {}: {:?}", key, value);
        }
        println!("Dependencies:");
        for (key, value) in &self.dependencies {
            println!("  {}: {:?}", key, value);
        }
        println!("Dependents:");
        for (key, value) in &self.dependents {
            println!("  {}: {:?}", key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::Status;
    use std::collections::HashSet;

    #[test]
    fn test_dependency_manager() {
        let mut dm = DependencyManagerImpl::<i32, StringDependency>::new();
        let x = StringDependency::new("x");
        let y = StringDependency::new("y");
        let z = StringDependency::new("z");
        dm.add_new(&x, 1, Status::Ready, &mut HashSet::new()).unwrap();
        dm.add_new(&y, 2, Status::Ready, &mut HashSet::new()).unwrap();
        dm.add_new(&z, 3, Status::Ready, &mut HashSet::new()).unwrap();
        assert_eq!(dm.all_dependents(&x).len(), 0);
        dm.add_raw_dependency(&x, &y, &mut HashSet::new()).unwrap();
        //dm.print();
        //println!("All dependents of x: {:?}", dm.all_dependents(&y));
        assert_eq!(dm.all_dependents(&y).len(), 1);
        dm.add_raw_dependency(&y, &z, &mut HashSet::new()).unwrap();
        assert_eq!(dm.all_dependents(&z).len(), 2);
        assert_eq!(dm.all_dependents(&z).contains(&y), true);
        assert_eq!(dm.all_dependents(&z).contains(&x), true);
    }

    #[test]
    fn test_expire() {
        let mut dm = DependencyManagerImpl::<i32, StringDependency>::new();
        let x = StringDependency::new("x");
        let y = StringDependency::new("y");
        let z = StringDependency::new("z");
        dm.add_new(&x, 1, Status::Ready, &mut HashSet::new()).unwrap();
        dm.add_new(&y, 2, Status::Ready, &mut HashSet::new()).unwrap();
        dm.add_new(&z, 3, Status::Ready, &mut HashSet::new()).unwrap();
        assert_eq!(dm.all_dependents(&x).len(), 0);
        dm.add_raw_dependency(&x, &y, &mut HashSet::new()).unwrap();
        dm.add_raw_dependency(&y, &z, &mut HashSet::new()).unwrap();
        
        let mut impacted = HashSet::new();
        dm.expire(&z, &mut impacted).unwrap();
        assert_eq!(impacted.len(), 3);
        
    }
}