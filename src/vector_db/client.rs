//! Qdrant client implementation

        use super::VectorStore;
        use super::models::{ContextLevel, Payload, VectorPoint, SearchParams, SearchResult, Filter as ModelFilter, Condition as ModelCondition};
        use crate::config::{VectorDbConfig, Distance};
        use crate::error::{VectorDbError, Result};
        use async_trait::async_trait;
        use qdrant_client::Qdrant;
        use qdrant_client::qdrant::{
            CreateCollectionBuilder, VectorParamsBuilder, VectorsConfig, PointStruct,
            SearchPoints, WithPayloadSelector, PointId, Value, Filter as QdrantFilter, 
            Condition as QdrantCondition, Range,
        };
        use qdrant_client::qdrant::vectors_config::Config;
        use qdrant_client::qdrant::with_payload_selector::SelectorOptions;
        use std::collections::HashMap;
        use tracing::{debug, info};
        use uuid::Uuid;

        /// Client for Qdrant vector database
        pub struct VectorDbClient {
            config: VectorDbConfig,
            client: Qdrant,
        }

        impl VectorDbClient {
            /// Create a new vector database client
            pub async fn new(config: VectorDbConfig) -> Result<Self> {
                info!("Connecting to Qdrant at {}", config.url);
                
                let client = Qdrant::from_url(&config.url)
                    .build()
                    .map_err(|e| VectorDbError::ConnectionError(e.to_string()))?;

                Ok(Self { config, client })
            }
            
            /// Initialize collections for all context levels
            pub async fn initialize_collections(&self) -> Result<()> {
                info!("Initializing collections for all context levels");
                
                for level in &[ContextLevel::Immediate, ContextLevel::ShortTerm, ContextLevel::LongTerm] {
                    let collection_name = self.collection_name(*level);
                    
                    // Check if collection exists
                    let exists = self.client
                        .collection_info(collection_name.clone())
                        .await
                        .is_ok();
                    
                    if !exists {
                        info!("Creating collection: {}", collection_name);
                        self.create_collection(&collection_name).await?;
                    } else {
                        debug!("Collection already exists: {}", collection_name);
                    }
                }
                
                Ok(())
            }
            
            /// Get collection name for a context level
            pub fn collection_name(&self, level: ContextLevel) -> String {
                format!("{}_{}", self.config.collection_prefix, level.as_str().to_lowercase())
            }
            
            /// Convert Distance enum to Qdrant Distance
            fn to_qdrant_distance(&self) -> qdrant_client::qdrant::Distance {
                match self.config.distance {
                    Distance::Cosine => qdrant_client::qdrant::Distance::Cosine,
                    Distance::Euclidean => qdrant_client::qdrant::Distance::Euclid,
                    Distance::Dot => qdrant_client::qdrant::Distance::Dot,
                }
            }
            
            /// Convert Payload to Qdrant payload
            fn to_qdrant_payload(&self, payload: &Payload) -> HashMap<String, Value> {
                let mut map = HashMap::new();
                
                map.insert("text".to_string(), Value::from(payload.text.clone()));
                map.insert("level".to_string(), Value::from(payload.level.as_str().to_string()));
                map.insert("timestamp".to_string(), Value::from(payload.timestamp));
                map.insert("agent_id".to_string(), Value::from(payload.agent_id.clone()));
                
                if let Some(session_id) = &payload.session_id {
                    map.insert("session_id".to_string(), Value::from(session_id.clone()));
                }
                
                // Add additional metadata
                for (key, value) in &payload.metadata {
                    if let Ok(v) = serde_json::to_string(value) {
                        map.insert(key.clone(), Value::from(v));
                    }
                }
                
                map
            }
            
            /// Convert Qdrant payload to Payload
            fn parse_qdrant_payload(&self, payload: HashMap<String, Value>) -> Result<Payload> {
                let text = payload.get("text")
                    .and_then(|v| v.kind.as_ref())
                    .and_then(|kind| match kind {
                        qdrant_client::qdrant::value::Kind::StringValue(s) => Some(s.clone()),
                        _ => None,
                    })
                    .ok_or_else(|| VectorDbError::SearchError("Missing text field".to_string()))?;
                
                let level_str = payload.get("level")
                    .and_then(|v| v.kind.as_ref())
                    .and_then(|kind| match kind {
                        qdrant_client::qdrant::value::Kind::StringValue(s) => Some(s.clone()),
                        _ => None,
                    })
                    .ok_or_else(|| VectorDbError::SearchError("Missing level field".to_string()))?;
                
                let level = match level_str.as_str() {
                    "Immediate" => ContextLevel::Immediate,
                    "ShortTerm" => ContextLevel::ShortTerm,
                    "LongTerm" => ContextLevel::LongTerm,
                    _ => return Err(VectorDbError::SearchError(format!("Invalid level: {}", level_str)).into()),
                };
                
                let timestamp = payload.get("timestamp")
                    .and_then(|v| v.kind.as_ref())
                    .and_then(|kind| match kind {
                        qdrant_client::qdrant::value::Kind::IntegerValue(i) => Some(*i),
                        _ => None,
                    })
                    .ok_or_else(|| VectorDbError::SearchError("Missing timestamp field".to_string()))?;
                
                let agent_id = payload.get("agent_id")
                    .and_then(|v| v.kind.as_ref())
                    .and_then(|kind| match kind {
                        qdrant_client::qdrant::value::Kind::StringValue(s) => Some(s.clone()),
                        _ => None,
                    })
                    .ok_or_else(|| VectorDbError::SearchError("Missing agent_id field".to_string()))?;
                
                let session_id = payload.get("session_id")
                    .and_then(|v| v.kind.as_ref())
                    .and_then(|kind| match kind {
                        qdrant_client::qdrant::value::Kind::StringValue(s) => Some(s.clone()),
                        _ => None,
                    });
                
                let mut metadata = HashMap::new();
                for (key, value) in payload {
                    if !["text", "level", "timestamp", "agent_id", "session_id"].contains(&key.as_str()) {
                        if let Some(kind) = value.kind.as_ref() {
                            match kind {
                                qdrant_client::qdrant::value::Kind::StringValue(s) => {
                                    if let Ok(json_value) = serde_json::from_str(s) {
                                        metadata.insert(key, json_value);
                                    }
                                }
                                qdrant_client::qdrant::value::Kind::IntegerValue(i) => {
                                    metadata.insert(key, serde_json::Value::Number((*i).into()));
                                }
                                qdrant_client::qdrant::value::Kind::DoubleValue(d) => {
                                    metadata.insert(key, serde_json::Value::Number(serde_json::Number::from_f64(*d).unwrap_or_else(|| serde_json::Number::from(0))));
                                }
                                qdrant_client::qdrant::value::Kind::BoolValue(b) => {
                                    metadata.insert(key, serde_json::Value::Bool(*b));
                                }
                                _ => {}
                            }
                        }
                    }
                }
                
                Ok(Payload {
                    text,
                    level,
                    timestamp,
                    agent_id,
                    session_id,
                    metadata,
                })
            }
            
            /// Convert Filter to Qdrant Filter
            fn to_qdrant_filter(&self, filter: &ModelFilter) -> QdrantFilter {
                let mut must_conditions = Vec::new();
                let mut should_conditions = Vec::new();
                let mut must_not_conditions = Vec::new();
                
                // Convert must conditions
                for condition in &filter.must {
                    if let Some(qc) = self.to_qdrant_condition(condition) {
                        must_conditions.push(qc);
                    }
                }
                
                // Convert should conditions
                for condition in &filter.should {
                    if let Some(qc) = self.to_qdrant_condition(condition) {
                        should_conditions.push(qc);
                    }
                }
                
                // Convert must_not conditions
                for condition in &filter.must_not {
                    if let Some(qc) = self.to_qdrant_condition(condition) {
                        must_not_conditions.push(qc);
                    }
                }
                
                QdrantFilter {
                    must: must_conditions,
                    should: should_conditions,
                    must_not: must_not_conditions,
                    min_should: None,
                }
            }
            
            /// Convert Condition to Qdrant Condition
            fn to_qdrant_condition(&self, condition: &ModelCondition) -> Option<QdrantCondition> {
                match condition {
                    ModelCondition::Match { key, value } => {
                        if let Some(s) = value.as_str() {
                            Some(QdrantCondition::matches(key.clone(), s.to_string()))
                        } else {
                            value.as_i64().map(|i| QdrantCondition::matches(key.clone(), i))
                        }
                    }
                    ModelCondition::Range { key, gte, lte } => {
                        let mut range_builder = Range::default();
                        if let Some(gte_val) = gte {
                            range_builder.gte = Some(*gte_val);
                        }
                        if let Some(lte_val) = lte {
                            range_builder.lte = Some(*lte_val);
                        }
                        
                        Some(QdrantCondition::range(key.clone(), range_builder))
                    }
                    ModelCondition::HasId { ids } => {
                        let point_ids: Vec<PointId> = ids.iter()
                            .map(|uuid| PointId::from(uuid.to_string()))
                            .collect();
                        
                        Some(QdrantCondition::has_id(point_ids))
                    }
                }
            }
        }
        
        #[async_trait]
        impl VectorStore for VectorDbClient {
            async fn create_collection(&self, name: &str) -> Result<()> {
                debug!("Creating collection: {}", name);
                
                let vector_params = VectorParamsBuilder::new(
                    self.config.vector_size as u64,
                    self.to_qdrant_distance(),
                ).build();

                self.client
                    .create_collection(
                        CreateCollectionBuilder::new(name)
                            .vectors_config(VectorsConfig {
                                config: Some(Config::Params(vector_params)),
                            })
                    )
                    .await
                    .map_err(|e| VectorDbError::ConnectionError(e.to_string()))?;
                
                info!("Collection created: {}", name);
                Ok(())
            }
            
            async fn delete_collection(&self, name: &str) -> Result<()> {
                debug!("Deleting collection: {}", name);
                
                self.client
                    .delete_collection(name)
                    .await
                    .map_err(|e| VectorDbError::ConnectionError(e.to_string()))?;
                
                info!("Collection deleted: {}", name);
                Ok(())
            }
            
            async fn insert_points(&self, collection: &str, points: Vec<VectorPoint>) -> Result<()> {
                if points.is_empty() {
                    return Ok(());
                }
                
                debug!("Inserting {} points into collection: {}", points.len(), collection);
                
                let qdrant_points: Vec<PointStruct> = points
                    .into_iter()
                    .map(|point| {
                        PointStruct::new(
                            point.id.to_string(),
                            point.vector,
                            self.to_qdrant_payload(&point.payload),
                        )
                    })
                    .collect();
                
                let upsert_points = qdrant_client::qdrant::UpsertPointsBuilder::new(
                    collection.to_string(),
                    qdrant_points,
                ).build();

                self.client
                    .upsert_points(upsert_points)
                    .await
                    .map_err(|e| VectorDbError::InsertError(e.to_string()))?;
                
                debug!("Points inserted successfully");
                Ok(())
            }
            
            async fn search(&self, collection: &str, params: SearchParams) -> Result<Vec<SearchResult>> {
                debug!("Searching in collection: {} with limit: {}", collection, params.limit);
                
                let mut search_points = SearchPoints {
                    collection_name: collection.to_string(),
                    vector: params.vector,
                    limit: params.limit as u64,
                    with_payload: Some(WithPayloadSelector {
                        selector_options: Some(SelectorOptions::Enable(params.with_payload)),
                    }),
                    with_vectors: Some(params.with_vector.into()),
                    score_threshold: params.score_threshold,
                    ..Default::default()
                };
                
                if let Some(filter) = params.filter {
                    search_points.filter = Some(self.to_qdrant_filter(&filter));
                }
                
                let results = self.client
                    .search_points(search_points)
                    .await
                    .map_err(|e| VectorDbError::SearchError(e.to_string()))?;
                
                let search_results: Result<Vec<SearchResult>> = results
                    .result
                    .into_iter()
                    .map(|point| {
                        let point_id = point.id.unwrap().point_id_options.unwrap();
                        let id_str = match point_id {
                            qdrant_client::qdrant::point_id::PointIdOptions::Num(num) => num.to_string(),
                            qdrant_client::qdrant::point_id::PointIdOptions::Uuid(uuid) => uuid,
                        };
                        let id = Uuid::parse_str(&id_str)
                            .map_err(|e| {
                                // Provide better error context for numeric IDs
                                if id_str.chars().all(|c| c.is_ascii_digit()) {
                                    VectorDbError::SearchError(format!("Invalid UUID from numeric ID {}: {}", id_str, e))
                                } else {
                                    VectorDbError::SearchError(format!("Invalid UUID: {}", e))
                                }
                            })?;
                        
                        let payload = if params.with_payload && !point.payload.is_empty() {
                            Some(self.parse_qdrant_payload(point.payload)?)
                        } else {
                            None
                        };
                        
                        let vector = if params.with_vector {
                            point.vectors.and_then(|v| {
                                if let Some(qdrant_client::qdrant::vectors_output::VectorsOptions::Vector(vec)) = v.vectors_options {
                                    Some(vec.data)
                                } else {
                                    None
                                }
                            })
                        } else {
                            None
                        };
                        
                        Ok(SearchResult {
                            id,
                            score: point.score,
                            payload,
                            vector,
                        })
                    })
                    .collect();
                
                let results = search_results?;
                debug!("Found {} results", results.len());
                Ok(results)
            }
            
            async fn delete_points(&self, collection: &str, ids: Vec<Uuid>) -> Result<()> {
                if ids.is_empty() {
                    return Ok(());
                }
                
                debug!("Deleting {} points from collection: {}", ids.len(), collection);
                
                let point_ids: Vec<PointId> = ids.iter()
                    .map(|uuid| PointId::from(uuid.to_string()))
                    .collect();
                
                let delete_points = qdrant_client::qdrant::DeletePointsBuilder::new(collection.to_string())
                    .points(point_ids)
                    .build();

                self.client
                    .delete_points(delete_points)
                    .await
                    .map_err(|e| VectorDbError::DeleteError(e.to_string()))?;
                
                debug!("Points deleted successfully");
                Ok(())
            }
            
            async fn get_point(&self, collection: &str, id: Uuid) -> Result<Option<VectorPoint>> {
                debug!("Getting point {} from collection: {}", id, collection);
                
                let point_id = PointId::from(id.to_string());
                
                let get_points = qdrant_client::qdrant::GetPointsBuilder::new(collection.to_string(), vec![point_id])
                    .with_payload(true)
                    .with_vectors(true)
                    .build();
                
                let points = self.client
                    .get_points(get_points)
                    .await
                    .map_err(|e| VectorDbError::SearchError(e.to_string()))?;
                
                if let Some(point) = points.result.first() {
                    let payload = self.parse_qdrant_payload(point.payload.clone())?;
                    
                    let vector = point.vectors.as_ref().and_then(|v| {
                        if let Some(qdrant_client::qdrant::vectors_output::VectorsOptions::Vector(vec)) = &v.vectors_options {
                            Some(vec.data.clone())
                        } else {
                            None
                        }
                    }).ok_or_else(|| VectorDbError::SearchError("Missing vector".to_string()))?;
                    
                    Ok(Some(VectorPoint {
                        id,
                        vector,
                        payload,
                    }))
                } else {
                    Ok(None)
                }
            }
        }