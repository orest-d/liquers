use std::{collections::HashMap, hash::Hash};

use serde_json::Value;

use crate::{
    command_metadata::CommandMetadataRegistry, context::{NGEnvRef, NGEnvironment}, error::Error, parse::parse_query, plan::{Plan, PlanBuilder}, query::{Key, Query, ResourceName}
};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Recipe {
    pub query: String,
    pub title: String,
    pub description: String,
    pub arguments: HashMap<String, Value>,
    pub links: HashMap<String, String>,
}

impl Recipe {
    pub fn new(query: String, title: String, description: String) -> Result<Recipe, Error> {
        Ok(Recipe {
            query: parse_query(&query)?.encode(),
            title,
            description,
            arguments: HashMap::new(),
            links: HashMap::new(),
        })
    }

    pub fn with_argument(mut self, name: String, value: Value) -> Self {
        self.arguments.insert(name, value);
        self
    }

    pub fn with_link(mut self, name: String, value: String) -> Self {
        self.links.insert(name, value);
        self
    }

    pub fn get_query(&self) -> Result<Query, Error> {
        parse_query(&self.query)
    }

    pub fn filename(&self) -> Result<ResourceName, Error> {
        self.get_query()?.filename().map_or(
            Err(Error::general_error(format!(
                "Recipe query {} lacks a filename",
                self.query
            ))),
            |filename| Ok(filename),
        )
    }

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


pub trait AsyncRecipeProvider {
    async fn assets_with_recipes(&self, key:&Key) -> Result<Vec<ResourceName>, Error>;
    async fn recipe_plan(&self, key:&Key) -> Result<Plan, Error>;
    async fn recipe(&self, key:&Key) -> Result<Recipe, Error>;    
}

pub struct DefaultRecipeProvider<E:NGEnvironment> {
    envref:NGEnvRef<E>,
}

impl<E:NGEnvironment> DefaultRecipeProvider<E> {
    pub fn new(envref:NGEnvRef<E>) -> Self {
        DefaultRecipeProvider{envref}
    }
    pub async fn get_recipes(&self, key:&Key) -> Result<RecipeList, Error> {
        self.envref.get_async_store().await.get_bytes(&key.join("recipes.yaml")).await.map_or(
            Err(Error::general_error(format!("No recipes found for key {}", key))),
            |bytes| serde_yaml::from_slice(&bytes).map_err(|e| Error::general_error(format!("Error parsing recipes: {}", e))),
        )
    }
}

impl<E:NGEnvironment> AsyncRecipeProvider for DefaultRecipeProvider<E> {
    async fn assets_with_recipes(&self, key:&Key) -> Result<Vec<ResourceName>, Error> {
        let recipes = self.get_recipes(key).await?;
        let mut assets = Vec::new();
        for recipe in recipes.recipes {
            if let Ok(filename) = recipe.filename(){
                assets.push(filename);
            }
        }
        Ok(assets)
    }

    async fn recipe_plan(&self, key:&Key) -> Result<Plan, Error> {
        if let Some(filename) = key.filename() {
            let recipes = self.get_recipes(&key.parent()).await?;
            let recipe = recipes.get(&filename.name).ok_or(Error::general_error(format!("No recipe found for key {}", key)).with_key(key))?;
            let env = self.envref.0.read().await;
            recipe.to_plan(env.get_command_metadata_registry()).map_err(|e| e.with_key(key))
        }
        else{
            return Err(Error::general_error(format!("No filename in key '{}'", key)).with_key(key));
        }
    }
    
    async fn recipe(&self, key:&Key) -> Result<Recipe, Error> {
        if let Some(filename) = key.filename() {
            let recipes = self.get_recipes(&key.parent()).await?;
            recipes.get(&filename.name).map_or(
                Err(Error::general_error(format!("No recipe found for key {}", key)).with_key(key)),
                |recipe| Ok(recipe.clone())
            )
        }
        else{
            return Err(Error::general_error(format!("No filename in key '{}'", key)).with_key(key));
        }
    }    
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RecipeList {
    pub recipes: Vec<Recipe>,
}

impl RecipeList {
    pub fn new() -> Self {
        RecipeList { recipes: Vec::new() }
    }

    pub fn add_recipe(&mut self, recipe: Recipe) {
        self.recipes.push(recipe);
    }

    pub fn len(&self) -> usize {
        self.recipes.len()
    }

    pub fn get(&self, name:&str) -> Option<&Recipe> {
        self.recipes.iter().find(|r| {
            if let Ok(filename) = r.filename(){
                filename.name == name
            }
            else{
                false
            }
        })
    }
}

#[cfg(test)]
mod test {
    use crate::{command_metadata::{ArgumentInfo, CommandMetadata, CommandMetadataRegistry}, plan::{ParameterValue, Step}};

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
        if let Step::Action { realm, ns, action_name, position, parameters } = &plan[0]{
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
}
