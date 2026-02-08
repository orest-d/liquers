# UI Payload Design

## Overview

This document defines the payload architecture for UI commands in Liquers. The design separates concerns:
- **UIPayload trait**: Generic interface that any payload can implement to provide UI context
- **UIAppState trait**: Interface for accessing/manipulating UI application state
- **UIHandle**: Type-safe handle for UI elements, directly injectable into commands

This allows users to create custom payload types that combine UI context with other responsibilities (e.g., user session, permissions, application-specific data).

---

## Core Types

### Location: `liquers-lib/src/ui/payload.rs`

```rust
use std::sync::Arc;
use liquers_core::commands::{PayloadType, ExtractFromPayload, InjectedFromContext};
use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;

/// Type-safe handle for UI elements
///
/// Directly injectable into commands via the `injected` keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UIHandle(pub u64);

impl std::fmt::Display for UIHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "UIHandle({})", self.0)
    }
}

/// Trait for UI application state
///
/// Provides methods to access and manipulate UI elements.
/// Implementations handle the actual storage and management of UI state.
pub trait UIAppState: Send + Sync {
    /// Get element by handle
    fn get_element(&self, handle: UIHandle) -> Result<Arc<RwLock<UiElement>>, Error>;

    /// Create new UI element
    fn create_element(
        &self,
        parent: Option<UIHandle>,
        element_type: ElementType,
        query: Query,
    ) -> Result<UIHandle, Error>;

    /// List child element handles
    fn list_children(&self, handle: UIHandle) -> Result<Vec<UIHandle>, Error>;

    /// Get root window handles
    fn root_windows(&self) -> Result<Vec<UIHandle>, Error>;

    /// Remove element (optional - not all implementations need this)
    fn remove_element(&self, handle: UIHandle) -> Result<(), Error> {
        Err(Error::general_error("Element removal not supported".to_string()))
    }
}

/// Trait that payloads must implement to support UI commands
///
/// This allows user-defined payload types to provide UI context alongside
/// other application-specific data (e.g., user session, permissions).
///
/// # Example
///
/// ```rust
/// #[derive(Clone)]
/// pub struct AppPayload {
///     current_ui_handle: Option<UIHandle>,
///     ui_state: Arc<DirectUIAppState>,
///     user_id: String,  // Application-specific field
///     session: String,  // Application-specific field
/// }
///
/// impl PayloadType for AppPayload {}
///
/// impl UIPayload for AppPayload {
///     fn handle(&self) -> Option<UIHandle> {
///         self.current_ui_handle
///     }
///
///     fn app_state(&self) -> Arc<dyn UIAppState> {
///         self.ui_state.clone()
///     }
/// }
/// ```
pub trait UIPayload: PayloadType {
    /// Get the current UI element handle
    ///
    /// Returns `None` if:
    /// - Payload is available but UI is not active (e.g., background task)
    /// - No UI element is currently focused
    fn handle(&self) -> Option<UIHandle>;

    /// Get the UI application state
    ///
    /// Returns an Arc to allow sharing across threads and cheap cloning.
    fn app_state(&self) -> Arc<dyn UIAppState>;
}
```

---

## UIHandle Injection

### Direct Injection (Generic)

UIHandle can be injected from any payload implementing UIPayload:

```rust
/// Generic extraction from any UIPayload
impl<P: UIPayload> ExtractFromPayload<P> for UIHandle {
    fn extract_from_payload(payload: &P) -> Result<Self, Error> {
        payload.handle()
            .ok_or_else(|| Error::general_error(
                "No current UI handle available in payload".to_string()
            ))
    }
}

/// Generic injection from any environment with UIPayload
impl<E: Environment> InjectedFromContext<E> for UIHandle
where
    E::Payload: UIPayload,
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload available".to_string()))?;
        UIHandle::extract_from_payload(&payload)
    }
}
```

**Usage in commands:**
```rust
fn set_query(handle: UIHandle, new_query: Query) -> Result<Value, Error> {
    // handle is automatically extracted from payload
    // Works with ANY payload implementing UIPayload!
    Ok(Value::none())
}

register_command!(cr, fn set_query(
    handle: UIHandle injected,
    new_query: Query
) -> result)?;
```

---

## Phase 1 Implementation

### Location: `liquers-lib/src/ui/app_state.rs`

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;
use std::collections::HashMap;

/// Simple in-memory implementation of UIAppState
pub struct DirectUIAppState {
    elements: Arc<RwLock<HashMap<UIHandle, Arc<RwLock<UiElement>>>>>,
    next_id: Arc<AtomicU64>,
}

impl DirectUIAppState {
    pub fn new() -> Self {
        Self {
            elements: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    fn generate_handle(&self) -> UIHandle {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        UIHandle(id)
    }
}

impl UIAppState for DirectUIAppState {
    fn get_element(&self, handle: UIHandle) -> Result<Arc<RwLock<UiElement>>, Error> {
        self.elements.blocking_read()
            .get(&handle)
            .cloned()
            .ok_or_else(|| Error::general_error(format!("Element not found: {}", handle)))
    }

    fn create_element(
        &self,
        parent: Option<UIHandle>,
        element_type: ElementType,
        query: Query,
    ) -> Result<UIHandle, Error> {
        let handle = self.generate_handle();

        let element = UiElement {
            id: handle,
            element_type,
            query,
            children: vec![],
            parent,
            widget_cache: None,
        };

        let element_arc = Arc::new(RwLock::new(element));
        self.elements.blocking_write().insert(handle, element_arc.clone());

        // If has parent, add to parent's children list
        if let Some(parent_handle) = parent {
            if let Ok(parent_elem) = self.get_element(parent_handle) {
                parent_elem.blocking_write().children.push(handle);
            }
        }

        Ok(handle)
    }

    fn list_children(&self, handle: UIHandle) -> Result<Vec<UIHandle>, Error> {
        let element = self.get_element(handle)?;
        Ok(element.blocking_read().children.clone())
    }

    fn root_windows(&self) -> Result<Vec<UIHandle>, Error> {
        let elements = self.elements.blocking_read();
        Ok(elements.iter()
            .filter(|(_, elem)| {
                let elem = elem.blocking_read();
                elem.parent.is_none() && matches!(elem.element_type, ElementType::Window { .. })
            })
            .map(|(handle, _)| *handle)
            .collect())
    }

    fn remove_element(&self, handle: UIHandle) -> Result<(), Error> {
        let mut elements = self.elements.blocking_write();

        // Remove from parent's children list
        if let Some(elem_arc) = elements.get(&handle) {
            let elem = elem_arc.blocking_read();
            if let Some(parent_handle) = elem.parent {
                if let Some(parent_arc) = elements.get(&parent_handle) {
                    parent_arc.blocking_write().children.retain(|h| *h != handle);
                }
            }
        }

        // Remove the element itself
        elements.remove(&handle)
            .ok_or_else(|| Error::general_error(format!("Element not found: {}", handle)))?;

        Ok(())
    }
}
```

---

## Concrete Payload Implementation

### Minimal UI-Only Payload

```rust
/// Simple payload for UI-only applications
#[derive(Clone)]
pub struct SimpleUIPayload {
    current_handle: Option<UIHandle>,
    app_state: Arc<DirectUIAppState>,
}

impl SimpleUIPayload {
    pub fn new(app_state: Arc<DirectUIAppState>) -> Self {
        Self {
            current_handle: None,
            app_state,
        }
    }

    pub fn with_handle(mut self, handle: UIHandle) -> Self {
        self.current_handle = Some(handle);
        self
    }

    pub fn set_current_handle(&mut self, handle: Option<UIHandle>) {
        self.current_handle = handle;
    }
}

impl PayloadType for SimpleUIPayload {}

impl UIPayload for SimpleUIPayload {
    fn handle(&self) -> Option<UIHandle> {
        self.current_handle
    }

    fn app_state(&self) -> Arc<dyn UIAppState> {
        self.app_state.clone()
    }
}

// Required for injection
impl<E: Environment<Payload = SimpleUIPayload>> InjectedFromContext<E> for SimpleUIPayload {
    fn from_context(name: &str, context: Context<E>) -> Result<Self, Error> {
        context.get_payload_clone().ok_or(Error::general_error(format!(
            "No payload in context for injected parameter {}", name
        )))
    }
}
```

### Extended Payload with Application Data

```rust
/// Payload that combines UI context with application-specific data
#[derive(Clone)]
pub struct AppPayload {
    // UI context
    current_handle: Option<UIHandle>,
    app_state: Arc<DirectUIAppState>,

    // Application-specific data
    user_id: String,
    session_id: String,
    permissions: Arc<HashSet<String>>,
}

impl AppPayload {
    pub fn new(
        app_state: Arc<DirectUIAppState>,
        user_id: String,
        session_id: String,
    ) -> Self {
        Self {
            current_handle: None,
            app_state,
            user_id,
            session_id,
            permissions: Arc::new(HashSet::new()),
        }
    }

    // Application-specific getters
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

impl PayloadType for AppPayload {}

impl UIPayload for AppPayload {
    fn handle(&self) -> Option<UIHandle> {
        self.current_handle
    }

    fn app_state(&self) -> Arc<dyn UIAppState> {
        self.app_state.clone()
    }
}

// Required for injection
impl<E: Environment<Payload = AppPayload>> InjectedFromContext<E> for AppPayload {
    fn from_context(name: &str, context: Context<E>) -> Result<Self, Error> {
        context.get_payload_clone().ok_or(Error::general_error(format!(
            "No payload in context for injected parameter {}", name
        )))
    }
}

// Optional: Define newtypes for application-specific field injection
pub struct UserId(pub String);

impl ExtractFromPayload<AppPayload> for UserId {
    fn extract_from_payload(payload: &AppPayload) -> Result<Self, Error> {
        Ok(UserId(payload.user_id.clone()))
    }
}

impl InjectedFromContext<MyAppEnvironment> for UserId {
    fn from_context(_name: &str, context: Context<MyAppEnvironment>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload".to_string()))?;
        UserId::extract_from_payload(&payload)
    }
}
```

---

## Updated UiElement

### Location: `liquers-lib/src/ui/element.rs`

```rust
use liquers_core::query::Query;
use serde::{Serialize, Deserialize};

/// UI element in application state tree
pub struct UiElement {
    pub id: UIHandle,
    pub element_type: ElementType,
    pub query: Query,
    pub children: Vec<UIHandle>,  // Changed from String to UIHandle
    pub parent: Option<UIHandle>,  // Changed from String to UIHandle
    pub widget_cache: Option<Box<dyn Widget>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ElementType {
    Window { title: String },
    Pane,
    Container,
}
```

---

## Command Examples

### Using UIHandle Injection

```rust
/// Update current element's query
fn set_query(handle: UIHandle, new_query: Query) -> Result<Value, Error> {
    // handle is automatically injected from any UIPayload
    // But we need app_state too - see next example
    Ok(Value::none())
}
```

### Using Full Payload

```rust
/// Update current element's query (complete version)
fn set_query<P: UIPayload>(payload: P, new_query: Query) -> Result<Value, Error> {
    let handle = payload.handle()
        .ok_or_else(|| Error::general_error("No current UI element".to_string()))?;

    let app_state = payload.app_state();
    let element = app_state.get_element(handle)?;

    let mut elem = element.blocking_write();
    elem.query = new_query;
    elem.widget_cache = None;

    Ok(Value::none())
}

// Note: Generic over P: UIPayload won't work directly with register_command!
// Need concrete type:

fn set_query_concrete(payload: SimpleUIPayload, new_query: Query) -> Result<Value, Error> {
    let handle = payload.handle()
        .ok_or_else(|| Error::general_error("No current UI element".to_string()))?;

    let app_state = payload.app_state();
    let element = app_state.get_element(handle)?;

    let mut elem = element.blocking_write();
    elem.query = new_query;
    elem.widget_cache = None;

    Ok(Value::none())
}

register_command!(cr, fn set_query_concrete(
    payload: SimpleUIPayload injected,
    new_query: Query
) -> result
    namespace: "ui"
    doc: "Update current element's query"
)?;
```

### Helper: Extracting AppState

For commands that need app_state but not handle:

```rust
pub struct AppState(pub Arc<dyn UIAppState>);

impl<P: UIPayload> ExtractFromPayload<P> for AppState {
    fn extract_from_payload(payload: &P) -> Result<Self, Error> {
        Ok(AppState(payload.app_state()))
    }
}

impl<E: Environment> InjectedFromContext<E> for AppState
where
    E::Payload: UIPayload,
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload".to_string()))?;
        AppState::extract_from_payload(&payload)
    }
}

// Usage:
fn create_window(app_state: AppState, title: String) -> Result<Value, Error> {
    let handle = app_state.0.create_element(
        None,
        ElementType::Window { title },
        Query::empty(),
    )?;
    Ok(Value::from(handle.0 as i64))
}

register_command!(cr, fn create_window(
    app_state: AppState injected,
    title: String
) -> result)?;
```

---

## Environment Setup

```rust
use liquers_core::context::SimpleEnvironmentWithPayload;
use liquers_core::value::Value;

pub type UIEnvironment = SimpleEnvironmentWithPayload<Value, SimpleUIPayload>;

pub fn create_ui_environment() -> UIEnvironment {
    let mut env = UIEnvironment::new();
    register_ui_commands(&mut env).expect("Failed to register UI commands");
    env
}

pub fn register_ui_commands(env: &mut UIEnvironment) -> Result<(), Error> {
    let cr = &mut env.command_registry;

    register_command!(cr, fn set_query_concrete(
        payload: SimpleUIPayload injected,
        new_query: Query
    ) -> result
        namespace: "ui"
        doc: "Update current element's query"
    )?;

    // More commands...

    Ok(())
}
```

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ui_handle_injection() {
        let mut env = UIEnvironment::new();
        register_ui_commands(&mut env).unwrap();

        let app_state = Arc::new(DirectUIAppState::new());

        // Create a test element
        let handle = app_state.create_element(
            None,
            ElementType::Window { title: "Test".to_string() },
            Query::empty(),
        ).unwrap();

        // Create payload with current handle
        let payload = SimpleUIPayload::new(app_state.clone())
            .with_handle(handle);

        // Evaluate command with payload
        let envref = env.to_ref();
        let result = envref.evaluate_immediately(
            "/-/ui/set_query_concrete-\"/data/test\"",
            payload,
        ).await.unwrap();

        // Verify query was updated
        let element = app_state.get_element(handle).unwrap();
        let elem = element.blocking_read();
        assert_eq!(elem.query.encode(), "/data/test");
    }

    #[test]
    fn test_ui_handle_display() {
        let handle = UIHandle(42);
        assert_eq!(format!("{}", handle), "UIHandle(42)");
    }

    #[test]
    fn test_app_state_create_element() {
        let app_state = DirectUIAppState::new();

        let handle = app_state.create_element(
            None,
            ElementType::Window { title: "Test".to_string() },
            Query::empty(),
        ).unwrap();

        let element = app_state.get_element(handle).unwrap();
        let elem = element.blocking_read();
        assert_eq!(elem.id, handle);
    }
}
```

---

## Design Benefits

1. **Flexibility**: Users can create custom payload types combining UI with application data
2. **Type Safety**: UIHandle is a distinct type, preventing confusion with other IDs
3. **Generic Injection**: UIHandle works with ANY payload implementing UIPayload
4. **Separation of Concerns**: UIAppState trait separates storage from payload concerns
5. **Optional UI Context**: Payload can exist without UI (handle returns None)

---

## Migration Path

For users with existing payloads:

```rust
// Existing payload
#[derive(Clone)]
struct MyAppPayload {
    user_session: String,
    // ... other fields
}

// Add UI support
impl MyAppPayload {
    ui_handle: Option<UIHandle>,
    ui_state: Arc<DirectUIAppState>,
}

impl UIPayload for MyAppPayload {
    fn handle(&self) -> Option<UIHandle> {
        self.ui_handle
    }

    fn app_state(&self) -> Arc<dyn UIAppState> {
        self.ui_state.clone()
    }
}

// Now UIHandle automatically works in commands!
```

---

*Design Document Version: 1.0*
*Date: 2026-01-31*
