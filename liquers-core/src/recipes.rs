use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;

use crate::{
    command_metadata::CommandMetadataRegistry,
    context::{NGEnvRef, NGEnvironment},
    error::Error,
    parse::parse_query,
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
        Ok("binary".to_string())
    }

    /// Converts the recipe to a `Plan` using the provided `CommandMetadataRegistry`.
    /// It applies the arguments and links to the plan.
    pub fn to_plan(&self, cmr: &CommandMetadataRegistry) -> Result<Plan, Error> {
        let query = self.get_query()?;
        let mut planbuilder = PlanBuilder::new(query.clone(), cmr).with_placeholders_allowed();
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
}

#[async_trait]
pub trait AsyncRecipeProvider: Send + Sync {
    /// Returns true if folder represented by key has recipes
    async fn has_recipes(&self, key: &Key) -> Result<bool, Error>;
    /// Returns a list of assets that have recipes in the folder represented by key
    async fn assets_with_recipes(&self, key: &Key) -> Result<Vec<ResourceName>, Error>;
    /// Returns the plan for the asset represented by key
    async fn recipe_plan(&self, key: &Key) -> Result<Plan, Error>;
    /// Returns the recipe for the asset represented by key
    async fn recipe(&self, key: &Key) -> Result<Recipe, Error>;
    /// Returns true if the asset represented by key has a recipe
    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        if let Some(name) = key.filename() {
            let parent_key = key.parent();
            if self.has_recipes(&parent_key).await? {
                let recipes = self.assets_with_recipes(&parent_key).await?;
                return Ok(recipes.iter().any(|resourcename| resourcename == name));
            } else {
                return Ok(false);
            }
        } else {
            Ok(false)
        }
    }
}

pub struct DefaultRecipeProvider<E: NGEnvironment> {
    envref: NGEnvRef<E>,
}

impl<E: NGEnvironment> DefaultRecipeProvider<E> {
    pub fn new(envref: NGEnvRef<E>) -> Self {
        DefaultRecipeProvider { envref }
    }
    pub async fn get_recipes(&self, key: &Key) -> Result<RecipeList, Error> {
        self.envref
            .get_async_store()
            .await
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
            )
    }
}

#[async_trait]
impl<E: NGEnvironment> AsyncRecipeProvider for DefaultRecipeProvider<E> {
    async fn assets_with_recipes(&self, key: &Key) -> Result<Vec<ResourceName>, Error> {
        if self.has_recipes(key).await? {
            let recipes = self.get_recipes(key).await?;
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

    async fn recipe_plan(&self, key: &Key) -> Result<Plan, Error> {
        if let Some(filename) = key.filename() {
            let recipes = self.get_recipes(&key.parent()).await?;
            let recipe = recipes.get(&filename.name).ok_or(
                Error::general_error(format!("No recipe found for key {}", key)).with_key(key),
            )?;
            let env = self.envref.0.read().await;
            recipe
                .to_plan(env.get_command_metadata_registry())
                .map_err(|e| e.with_key(key))
        } else {
            return Err(
                Error::general_error(format!("No filename in key '{}'", key)).with_key(key),
            );
        }
    }

    async fn recipe(&self, key: &Key) -> Result<Recipe, Error> {
        if let Some(filename) = key.filename() {
            let recipes = self.get_recipes(&key.parent()).await?;
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

    async fn has_recipes(&self, key: &Key) -> Result<bool, Error> {
        self.envref
            .get_async_store()
            .await
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
}

#[cfg(test)]
mod test {
    use crate::{
        command_metadata::{ArgumentInfo, CommandMetadata, CommandMetadataRegistry},
        plan::{ParameterValue, Step},
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
            realm,
            ns,
            action_name,
            position,
            parameters,
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
}
