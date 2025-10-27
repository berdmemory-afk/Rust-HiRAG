//! Search utilities and helpers

use super::models::*;

/// Builder for constructing search queries
pub struct SearchQueryBuilder {
    vector: Vec<f32>,
    limit: usize,
    score_threshold: Option<f32>,
    filter: Option<Filter>,
    with_payload: bool,
    with_vector: bool,
}

impl SearchQueryBuilder {
    pub fn new(vector: Vec<f32>) -> Self {
        Self {
            vector,
            limit: 10,
            score_threshold: None,
            filter: None,
            with_payload: true,
            with_vector: false,
        }
    }
    
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
    
    pub fn score_threshold(mut self, threshold: f32) -> Self {
        self.score_threshold = Some(threshold);
        self
    }
    
    pub fn filter(mut self, filter: Filter) -> Self {
        self.filter = Some(filter);
        self
    }
    
    pub fn with_payload(mut self, with_payload: bool) -> Self {
        self.with_payload = with_payload;
        self
    }
    
    pub fn with_vector(mut self, with_vector: bool) -> Self {
        self.with_vector = with_vector;
        self
    }
    
    pub fn build(self) -> SearchParams {
        SearchParams {
            vector: self.vector,
            limit: self.limit,
            score_threshold: self.score_threshold,
            filter: self.filter,
            with_payload: self.with_payload,
            with_vector: self.with_vector,
        }
    }
}