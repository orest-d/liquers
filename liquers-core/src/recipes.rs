use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;

use crate::{
    command_metadata::CommandMetadataRegistry,
    context::{EnvRef, Environment},
    error::Error,
    metadata::{AssetInfo, Status},
    parse::{parse_key, parse_query},
    plan::{Plan, PlanBuilder},
    query::{Key, Query, ResourceName},
};

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Recipe {
    #[serde(skip_serializing_if = "String::is_empty")]
    #[serde(default)]
    pub query: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    #[serde(default)]
    pub title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    #[serde(default)]
    pub description: String,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(default)]
    pub arguments: HashMap<String, Value>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(default)]
    pub links: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub cwd: Option<String>,
    /// If true, the recipe is treated as volatile even if it doesn't have a volatile plan
    #[serde(skip_serializing_if = "is_false")]
    #[serde(default = "false_default")]
    pub volatile: bool,
}

fn is_false(b: &bool) -> bool {
    *b == false
}

fn false_default() -> bool {
    false
}

impl Recipe {
    /// Creates a new recipe with the given query, title, and description.
    pub fn new(query: String, title: String, description: String) -> Result<Recipe, Error> {
        Ok(Recipe {
            query: parse_query(&query)?.encode(),
            title,
            description,
            arguments: HashMap::new(),
            links: HashMap::new(),
            cwd: None,
            volatile: false,
        })
    }

    /// Specify an argument for the recipe.
    pub fn with_argument(mut self, name: String, value: Value) -> Self {
        self.arguments.insert(name, value);
        self
    }

    /// Specify a link for the recipe.
    pub fn with_link(mut self, name: String, value: String) -> Self {
        self.links.insert(name, value);
        self
    }

    /// Returns the query of the recipe as a `Query` object.
    pub fn get_query(&self) -> Result<Query, Error> {
        parse_query(&self.query)
    }

    /// Returns the filename of the recipe, if it has a valid query.
    /// NOTE: Though in general recipes need to have filenames,
    /// ad-hoc recipes (stemming e.g. from web API calls) of queries converted to recipes
    /// do not need to have a filename.
    pub fn filename(&self) -> Result<Option<ResourceName>, Error> {
        Ok(self.get_query()?.filename())
    }

    /// Return filename extension of the recipe, if it has a valid query.
    pub fn extension(&self) -> Result<Option<String>, Error> {
        Ok(self.get_query()?.extension())
    }

    /// Returns the data format of the recipe, which is the file extension if available, or "binary" otherwise.
    /// This is used to determine e.g. how the recipe result will be stored.
    /// Default is "binary" if no extension is available.
    pub fn data_format(&self) -> Result<String, Error> {
        if let Some(extension) = self.extension()? {
            return Ok(extension);
        }
        Ok("bin".to_string())
    }

    pub fn unicode_icon(&self) -> String {
        if let Ok(Some(extension)) = self.extension() {
            crate::icons::file_extension_to_unicode_icon(&extension).to_owned()
        } else {
            crate::icons::DEFAULT_ICON.to_owned()
        }
    }

    /// Returns true if the recipe has any arguments either values or links.
    pub fn has_arguments(&self) -> bool {
        !self.arguments.is_empty() || !self.links.is_empty()
    }
    /// Returns true if the recipe is a pure query (i.e. is a valid query with no arguments or links).
    pub fn is_pure_query(&self) -> bool {
        !self.has_arguments() && self.get_query().is_ok()
    }
    /// Returns key if the recipe is a pure query, that is a key.
    /// Key is expanded to an absolute form using the cwd (if set)
    pub fn key(&self) -> Result<Option<Key>, Error> {
        let query = self.get_query()?;
        if !self.has_arguments() {
            if let Some(key) = query.key() {
                if let Some(cwd) = &self.cwd {
                    let cwd = parse_key(cwd)?;
                    return Ok(Some(key.to_absolute(&cwd)));
                } else {
                    return Ok(Some(key));
                }
            }
        }
        Ok(None)
    }

    /// Converts the recipe to a `Plan` using the provided `CommandMetadataRegistry`.
    /// It applies the arguments and links to the plan.
    pub fn to_plan(&self, cmr: &CommandMetadataRegistry) -> Result<Plan, Error> {
        let query = self.get_query()?;
        let mut planbuilder = PlanBuilder::new(query.clone(), cmr).with_placeholders_allowed();
        //            .disable_expand_predecessors(); // TODO: fix - evaluate_immediately unittest is crashing with this option
        let mut plan = planbuilder.build()?;

        for (name, value) in &self.arguments {
            if !(plan.override_value(name, value.clone())) {
                return Err(Error::general_error(format!(
                    "Argument {} not found in last action",
                    name
                ))
                .with_query(&query));
            }
        }
        for (name, link) in &self.links {
            if !(plan.override_link(name, parse_query(link)?)) {
                return Err(Error::general_error(format!(
                    "Link {} not found in last action",
                    name
                ))
                .with_query(&query));
            }
        }
        Ok(plan)
    }

    /// Return current working directory for the recipe, if set.
    /// CWD is used to resolve relative keys in the recipe query and links.
    /// CWD is set automatically when loading recipes from a folder.
    /// This may raise an error if cwd is not a valid key.
    pub fn get_cwd(&self) -> Result<Option<Key>, Error> {
        if let Some(cwd) = &self.cwd {
            Ok(Some(parse_key(cwd)?))
        } else {
            Ok(None)
        }
    }

    /// Returns the key to which the result of the recipe should be stored, if applicable.
    pub fn store_to_key(&self) -> Result<Option<Key>, Error> {
        let filename = self.filename()?;
        let cwd = self.get_cwd()?;
        if let (Some(filename), Some(cwd)) = (filename, cwd) {
            Ok(Some(cwd.join(filename.name)))
        } else {
            Ok(None)
        }
    }

    pub fn get_asset_info(&self) -> Result<AssetInfo, Error> {
        let mut asset_info = AssetInfo::new();
        asset_info.key = None; // Key is not known to the recipe
        if self.is_pure_query() {
            asset_info.query = if let Ok(query) = self.get_query() {
                Some(query)
            } else {
                None
            };
        }
        asset_info.message = "Recipe available".to_string();
        asset_info.title = self.title.clone();
        asset_info.description = self.description.clone();
        asset_info.filename = self.filename()?.map(|f| f.name);
        asset_info.data_format = Some(self.data_format()?);
        asset_info.is_error = false;
        asset_info.is_dir = false;
        asset_info.status = Status::Recipe;
        asset_info.unicode_icon = self.unicode_icon();
        Ok(asset_info)
    }
}

impl From<&Query> for Recipe {
    fn from(query: &Query) -> Self {
        Recipe {
            query: query.encode(),
            title: "Ad-hoc query".to_string(),
            description: "".to_string(),
            arguments: HashMap::new(),
            links: HashMap::new(),
            cwd: None,
            volatile: false,
        }
    }
}

impl From<Query> for Recipe {
    fn from(query: Query) -> Self {
        Recipe {
            query: query.encode(),
            title: "Ad-hoc query".to_string(),
            description: "".to_string(),
            arguments: HashMap::new(),
            links: HashMap::new(),
            cwd: None,
            volatile: false,
        }
    }
}

impl From<Key> for Recipe {
    fn from(key: Key) -> Self {
        Recipe {
            query: Query::from(key).encode(),
            title: "Ad-hoc key-query".to_string(),
            description: "".to_string(),
            arguments: HashMap::new(),
            links: HashMap::new(),
            cwd: None,
            volatile: false,
        }
    }
}

impl From<&Key> for Recipe {
    fn from(key: &Key) -> Self {
        Recipe {
            query: Query::from(key).encode(),
            title: "Ad-hoc key-query".to_string(),
            description: "".to_string(),
            arguments: HashMap::new(),
            links: HashMap::new(),
            cwd: None,
            volatile: false,
        }
    }
}

#[async_trait]
pub trait AsyncRecipeProvider<E:Environment>: Send + Sync {
    /// Returns true if folder represented by key has recipes
    async fn has_recipes(&self, key: &Key, envref: EnvRef<E>) -> Result<bool, Error>;
    /// Returns a list of assets that have recipes in the folder represented by key
    async fn assets_with_recipes(&self, key: &Key, envref: EnvRef<E>) -> Result<Vec<ResourceName>, Error>;
    /// Returns the plan for the asset represented by key
    async fn recipe_plan(&self, key: &Key, envref: EnvRef<E>) -> Result<Plan, Error>;
    /// Returns the recipe for the asset represented by key
    /// Errors if no recipe is found
    async fn recipe(&self, key: &Key, envref: EnvRef<E>) -> Result<Recipe, Error>;
    /// Returns a recipe if available, None otherwise
    /// Error can still occur e.g. for an IO error.
    async fn recipe_opt(&self, key: &Key, envref: EnvRef<E>) -> Result<Option<Recipe>, Error>;
    /// Returns true if the asset represented by key has a recipe
    async fn contains(&self, key: &Key, envref: EnvRef<E>) -> Result<bool, Error> {
        if let Some(name) = key.filename() {
            let parent_key = key.parent();
            if self.has_recipes(&parent_key, envref.clone()).await? {
                let recipes = self.assets_with_recipes(&parent_key, envref.clone()).await?;
                return Ok(recipes.iter().any(|resourcename| resourcename == name));
            } else {
                return Ok(false);
            }
        } else {
            Ok(false)
        }
    }
    /// Returns asset info for the asset represented by key
    /// This is a true asset info only if the asset is not available.
    async fn get_asset_info(&self, key: &Key, envref: EnvRef<E>) -> Result<AssetInfo, Error> {
        let recipe = self.recipe(key, envref).await?;
        let mut asset_info = recipe.get_asset_info()?;
        asset_info.key = Some(key.clone());
        Ok(asset_info)
    }
}

pub struct TrivialRecipeProvider;

#[async_trait]
impl<E:Environment> AsyncRecipeProvider<E> for TrivialRecipeProvider {
    async fn assets_with_recipes(&self, _key: &Key, _envref:EnvRef<E>) -> Result<Vec<ResourceName>, Error> {
        Ok(Vec::new())
    }

    async fn recipe_plan(&self, key: &Key, _envref:EnvRef<E>) -> Result<Plan, Error> {
        return Err(
            Error::general_error(format!("No recipe plans defined by the trivial recipe provider; key '{}'", key)).with_key(key),
        );
    }

    async fn recipe(&self, key: &Key, _envref:EnvRef<E>) -> Result<Recipe, Error> {
        return Err(
            Error::general_error(format!("No recipes defined by the trivial recipe provider; key '{}'", key)).with_key(key),
        );
    }

    async fn recipe_opt(&self, _key: &Key, _envref:EnvRef<E>) -> Result<Option<Recipe>, Error> {
        Ok(None)
    }

    async fn has_recipes(&self, _key: &Key, _envref:EnvRef<E>) -> Result<bool, Error> {
        Ok(false)
    }
}

pub struct DefaultRecipeProvider;

impl DefaultRecipeProvider {
    pub async fn get_recipes<E:Environment>(&self, key: &Key, envref:EnvRef<E>) -> Result<RecipeList, Error> {
        let mut recipes: RecipeList = envref
            .get_async_store()
            .get_bytes(&key.join("recipes.yaml"))
            .await
            .map_or(
                Err(
                    Error::general_error(format!("No recipes found for folder {}", key))
                        .with_key(key),
                ),
                |bytes| {
                    serde_yaml::from_slice(&bytes)
                        .map_err(|e| Error::general_error(format!("Error parsing recipes: {}", e)))
                },
            )?;
        recipes.set_cwd(key.encode()).map_err(|e| e.with_key(key))?;
        Ok(recipes)
    }
}

#[async_trait]
impl<E: Environment> AsyncRecipeProvider<E> for DefaultRecipeProvider {
    async fn assets_with_recipes(&self, key: &Key, envref:EnvRef<E>) -> Result<Vec<ResourceName>, Error> {
        if self.has_recipes(key, envref.clone()).await? {
            let recipes = self.get_recipes(key, envref.clone()).await?;
            let mut assets = Vec::new();
            for recipe in recipes.recipes {
                if let Ok(Some(filename)) = recipe.filename() {
                    assets.push(filename);
                }
            }
            Ok(assets)
        } else {
            Ok(Vec::new())
        }
    }

    // TODO: Not used at the moment - consider removing
    /// Convenience method to get a plan for a recipe
    /// It fetches the recipe (if available) and uses [Recipe::to_plan] to convert it to a plan.
    async fn recipe_plan(&self, key: &Key, envref:EnvRef<E>) -> Result<Plan, Error> {
        if let Some(filename) = key.filename() {
            let recipes = self.get_recipes(&key.parent(), envref.clone()).await?;
            let recipe = recipes.get(&filename.name).ok_or(
                Error::general_error(format!("No recipe found for key {}", key)).with_key(key),
            )?;
            recipe
                .to_plan(envref.get_command_metadata_registry())
                .map_err(|e| e.with_key(key))
        } else {
            return Err(
                Error::general_error(format!("No filename in key '{}'", key)).with_key(key),
            );
        }
    }

    async fn recipe(&self, key: &Key, envref:EnvRef<E>) -> Result<Recipe, Error> {
        if let Some(filename) = key.filename() {
            let recipes = self.get_recipes(&key.parent(), envref).await?;
            recipes.get(&filename.name).map_or(
                Err(Error::general_error(format!("No recipe found for key {}", key)).with_key(key)),
                |recipe| Ok(recipe.clone()),
            )
        } else {
            return Err(
                Error::general_error(format!("No filename in key '{}'", key)).with_key(key),
            );
        }
    }

    async fn recipe_opt(&self, key: &Key, envref:EnvRef<E>) -> Result<Option<Recipe>, Error> {
        if let Some(filename) = key.filename() {
            let parent_key = key.parent();
            if self.has_recipes(&parent_key, envref.clone()).await? {
                let recipes = self.get_recipes(&parent_key, envref).await?;
                return Ok(recipes.get(&filename.name).cloned());
            }
        }
        Ok(None)
    }

    async fn has_recipes(&self, key: &Key, envref: EnvRef<E>) -> Result<bool, Error> {
        envref
            .get_async_store()
            .contains(&key.join("recipes.yaml"))
            .await
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RecipeList {
    pub recipes: Vec<Recipe>,
}

impl RecipeList {
    pub fn new() -> Self {
        RecipeList {
            recipes: Vec::new(),
        }
    }

    pub fn add_recipe(&mut self, recipe: Recipe) {
        self.recipes.push(recipe);
    }

    pub fn len(&self) -> usize {
        self.recipes.len()
    }

    pub fn get(&self, name: &str) -> Option<&Recipe> {
        self.recipes.iter().find(|r| {
            if let Ok(Some(filename)) = r.filename() {
                filename.name == name
            } else {
                false
            }
        })
    }

    /// Set the current working directory for all the recipes in the list that do not have the CWD set.
    pub fn set_cwd(&mut self, cwd: String) -> Result<(), Error> {
        for recipe in &mut self.recipes {
            if recipe.cwd.is_none() {
                recipe.cwd = Some(cwd.clone());
            } else {
                return Err(Error::not_supported(
                    "CWD can't be explicitly specified in a recipe".to_owned(),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::{
        command_metadata::{ArgumentInfo, CommandMetadata, CommandMetadataRegistry},
        plan::{ParameterValue, Step}, query::Key,
    };

    use super::RecipeList;

    #[test]
    fn empty_recipe() {
        let recipe = super::Recipe::new(
            "".to_string(),
            "title".to_string(),
            "description".to_string(),
        )
        .unwrap();
        assert_eq!(recipe.query, "".to_string());
        assert_eq!(recipe.title, "title".to_string());
        assert_eq!(recipe.description, "description".to_string());
        assert_eq!(recipe.arguments.len(), 0);
        assert_eq!(recipe.links.len(), 0);
        let plan = recipe
            .to_plan(&super::CommandMetadataRegistry::new())
            .unwrap();
        println!("plan: {:?}", &plan);
        println!("");
        println!("plan.yaml:\n{}", serde_yaml::to_string(&plan).unwrap());
        println!("");
    }
    #[test]
    fn recipe_with_parameter() {
        let mut cr = CommandMetadataRegistry::new();
        cr.add_command(CommandMetadata::new("a").with_argument(ArgumentInfo::any_argument("b")));
        let recipe = super::Recipe::new(
            "a".to_string(),
            "title".to_string(),
            "description".to_string(),
        )
        .unwrap()
        .with_argument("b".to_string(), serde_json::json!("c"));
        let plan = recipe.to_plan(&cr).unwrap();
        println!("plan.yaml:\n{}", serde_yaml::to_string(&plan).unwrap());
        println!("");
        assert!(plan.len() == 1);
        if let Step::Action {
            action_name,
            parameters,
            ..
        } = &plan[0]
        {
            assert!(action_name == "a");
            assert!(parameters.0.len() == 1);
            if let ParameterValue::OverrideValue(name, value) = &parameters.0[0] {
                assert!(name == "b");
                assert!(value == &serde_json::json!("c"));
            } else {
                assert!(false);
            }
        } else {
            assert!(false);
        }
    }

    #[test]
    fn recipefile() {
        let recipe = super::Recipe::new("a".to_string(), "test title".to_string(), "".to_string())
            .unwrap()
            .with_argument("b".to_string(), serde_json::json!("c"));
        let mut recipelist = RecipeList::new();
        recipelist.add_recipe(recipe);
        println!(
            "recipes.yaml:\n{}",
            serde_yaml::to_string(&recipelist).unwrap()
        );
    }

    #[cfg(feature = "async_store")]
    #[tokio::test]
    async fn test_default_recipe_provider() {
        use crate::context::{EnvRef, Environment, SimpleEnvironment};
        use crate::metadata::Metadata;
        use crate::parse::parse_key;
        use crate::store::{AsyncStoreWrapper, MemoryStore, Store};
        use crate::value::Value;

        // Create a MemoryStore and populate it with recipes.yaml
        let memory_store = MemoryStore::new(&Key::new());
        
        // Create a recipe list
        let mut recipe_list = RecipeList::new();
        recipe_list.add_recipe(
            super::Recipe::new(
                "-R/hello/test.txt".to_string(),
                "Test Recipe".to_string(),
                "A test recipe".to_string(),
            )
            .unwrap(),
        );
        recipe_list.add_recipe(
            super::Recipe::new(
                "-R/data/another.json".to_string(),
                "Another Recipe".to_string(),
                "Another test recipe".to_string(),
            )
            .unwrap(),
        );

        // Serialize to YAML
        let yaml_content = serde_yaml::to_string(&recipe_list).unwrap();
        println!("recipes.yaml content:\n{}", yaml_content);

        // Store the recipes.yaml in the MemoryStore at folder/recipes.yaml
        let recipes_key = parse_key("folder/recipes.yaml").unwrap();
        let metadata = Metadata::new();
        memory_store
            .set(&recipes_key, yaml_content.as_bytes(), &metadata)
            .unwrap();
        memory_store
            .set(&parse_key("hello/test.txt").unwrap(), "Hello, world!".as_bytes(), &metadata)
            .unwrap();

        // Wrap the MemoryStore with AsyncStoreWrapper
        let async_store = AsyncStoreWrapper(memory_store);

        // Create a SimpleEnvironment and set the async store
        let mut env = SimpleEnvironment::<Value>::new();
        env.with_async_store(Box::new(async_store));
        let envref: EnvRef<SimpleEnvironment<Value>> = env.to_ref();

        // Create a DefaultRecipeProvider
        let provider = super::DefaultRecipeProvider;

        // Test has_recipes
        let folder_key = parse_key("folder").unwrap();
        let has_recipes = super::AsyncRecipeProvider::has_recipes(&provider, &folder_key, envref.clone()).await.unwrap();
        assert!(has_recipes, "Should have recipes in folder");

        // Test get_recipes
        let recipes = provider.get_recipes(&folder_key, envref.clone()).await.unwrap();
        assert_eq!(recipes.len(), 2, "Should have 2 recipes");

        // Test assets_with_recipes
        let assets = super::AsyncRecipeProvider::assets_with_recipes(&provider, &folder_key, envref.clone()).await.unwrap();
        assert_eq!(assets.len(), 2, "Should have 2 assets with recipes");
        
        let asset_names: Vec<String> = assets.iter().map(|a| a.name.clone()).collect();
        assert!(asset_names.contains(&"test.txt".to_string()));
        assert!(asset_names.contains(&"another.json".to_string()));

        // Test recipe
        let test_recipe_key = parse_key("folder/test.txt").unwrap();
        let recipe = super::AsyncRecipeProvider::recipe(&provider, &test_recipe_key, envref.clone()).await.unwrap();
        assert_eq!(recipe.title, "Test Recipe");
        assert_eq!(recipe.description, "A test recipe");
        
        // Verify CWD was set correctly
        assert_eq!(recipe.cwd, Some("folder".to_string()));

        // Test recipe_opt with existing recipe
        let recipe_opt = super::AsyncRecipeProvider::recipe_opt(&provider, &test_recipe_key, envref.clone()).await.unwrap();
        assert!(recipe_opt.is_some());

        // Test recipe_opt with non-existing recipe
        let nonexistent_key = parse_key("folder/nonexistent.txt").unwrap();
        let recipe_opt = super::AsyncRecipeProvider::recipe_opt(&provider, &nonexistent_key, envref.clone()).await.unwrap();
        assert!(recipe_opt.is_none());

        // Test contains
        let contains = super::AsyncRecipeProvider::contains(&provider, &test_recipe_key, envref.clone()).await.unwrap();
        assert!(contains, "Should contain test.txt recipe");

        let not_contains = super::AsyncRecipeProvider::contains(&provider, &nonexistent_key, envref.clone()).await.unwrap();
        assert!(!not_contains, "Should not contain nonexistent.txt recipe");
    }

}
