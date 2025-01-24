use std::{collections::HashMap, hash::Hash};

use serde_json::Value;

use crate::{
    command_metadata::CommandMetadataRegistry, error::Error, parse::parse_query, plan::{Plan, PlanBuilder}, query::{Query, ResourceName}
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

    pub fn to_plan(&self, cmr:&CommandMetadataRegistry) -> Result<Plan, Error> {        
        let query = self.get_query()?;
        let mut planbuilder = PlanBuilder::new(query.clone(), cmr);
        for (name, value) in &self.arguments {
            if !(planbuilder.override_value(name, value.clone())) {
                return Err(Error::general_error(format!("Argument {} not found in last action", name)).with_query(&query));
            }
        }
        for (name, link) in &self.links {
            if !(planbuilder.override_link(name, parse_query(link)?)) {
                return Err(Error::general_error(format!("Link {} not found in last action", name)).with_query(&query));
            }
        }
        planbuilder.build()
    }

}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RecipeList {
    pub recipes: Vec<Recipe>,
}

#[cfg(test)]
mod test{
    #[test]
    fn empty_recipe() {
        let recipe = super::Recipe::new("".to_string(), "title".to_string(), "description".to_string()).unwrap();
        assert_eq!(recipe.query, "".to_string());
        assert_eq!(recipe.title, "title".to_string());
        assert_eq!(recipe.description, "description".to_string());
        assert_eq!(recipe.arguments.len(), 0);
        assert_eq!(recipe.links.len(), 0);
        let plan = recipe.to_plan(&super::CommandMetadataRegistry::new()).unwrap();
        println!("plan: {:?}", &plan);
        print!("");
        println!("plan.yaml:\n{}", serde_yaml::to_string(&plan).unwrap());
        print!("");
    }
}