use dozer_core::dag::app::App;
use dozer_core::dag::appsource::{AppSource, AppSourceManager};
use dozer_core::dag::channels::SourceChannelForwarder;
use dozer_core::dag::dag::DEFAULT_PORT_HANDLE;
use dozer_core::dag::epoch::Epoch;
use dozer_core::dag::errors::ExecutionError;
use dozer_core::dag::executor::{DagExecutor, ExecutorOptions};
use dozer_core::dag::node::{
    OutputPortDef, OutputPortType, PortHandle, Sink, SinkFactory, Source, SourceFactory,
};
use dozer_core::dag::record_store::RecordReader;
use dozer_core::storage::lmdb_storage::{LmdbEnvironmentManager, SharedTransaction};
use dozer_types::ordered_float::OrderedFloat;
use dozer_types::tracing::{debug, info};
use dozer_types::types::{
    Field, FieldDefinition, FieldType, Operation, Record, Schema, SourceDefinition,
};

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tempdir::TempDir;

use crate::pipeline::builder::{statement_to_pipeline, SchemaSQLContext};

const USER_PORT: u16 = 0 as PortHandle;
const DEPARTMENT_PORT: u16 = 1 as PortHandle;

#[derive(Debug)]
pub struct TestSourceFactory {
    running: Arc<AtomicBool>,
}

impl TestSourceFactory {
    pub fn new(running: Arc<AtomicBool>) -> Self {
        Self { running }
    }
}

impl SourceFactory<SchemaSQLContext> for TestSourceFactory {
    fn get_output_ports(&self) -> Result<Vec<OutputPortDef>, ExecutionError> {
        Ok(vec![
            OutputPortDef::new(
                USER_PORT,
                OutputPortType::StatefulWithPrimaryKeyLookup {
                    retr_old_records_for_updates: true,
                    retr_old_records_for_deletes: true,
                },
            ),
            OutputPortDef::new(
                DEPARTMENT_PORT,
                OutputPortType::StatefulWithPrimaryKeyLookup {
                    retr_old_records_for_updates: true,
                    retr_old_records_for_deletes: true,
                },
            ),
        ])
    }

    fn get_output_schema(
        &self,
        port: &PortHandle,
    ) -> Result<(Schema, SchemaSQLContext), ExecutionError> {
        if port == &USER_PORT {
            Ok((
                Schema::empty()
                    .field(
                        FieldDefinition::new(
                            String::from("id"),
                            FieldType::Int,
                            false,
                            SourceDefinition::Dynamic,
                        ),
                        true,
                    )
                    .field(
                        FieldDefinition::new(
                            String::from("name"),
                            FieldType::String,
                            false,
                            SourceDefinition::Dynamic,
                        ),
                        false,
                    )
                    .field(
                        FieldDefinition::new(
                            String::from("department_id"),
                            FieldType::Int,
                            false,
                            SourceDefinition::Dynamic,
                        ),
                        false,
                    )
                    .field(
                        FieldDefinition::new(
                            String::from("salary"),
                            FieldType::Float,
                            false,
                            SourceDefinition::Dynamic,
                        ),
                        false,
                    )
                    .clone(),
                SchemaSQLContext::default(),
            ))
        } else if port == &DEPARTMENT_PORT {
            Ok((
                Schema::empty()
                    .field(
                        FieldDefinition::new(
                            String::from("id"),
                            FieldType::Int,
                            false,
                            SourceDefinition::Dynamic,
                        ),
                        true,
                    )
                    .field(
                        FieldDefinition::new(
                            String::from("name"),
                            FieldType::String,
                            false,
                            SourceDefinition::Dynamic,
                        ),
                        false,
                    )
                    .clone(),
                SchemaSQLContext::default(),
            ))
        } else {
            panic!("Invalid Port Handle {port}");
        }
    }

    fn build(
        &self,
        _output_schemas: HashMap<PortHandle, Schema>,
    ) -> Result<Box<dyn Source>, ExecutionError> {
        Ok(Box::new(TestSource {
            running: self.running.clone(),
        }))
    }

    fn prepare(
        &self,
        _output_schemas: HashMap<PortHandle, (Schema, SchemaSQLContext)>,
    ) -> Result<(), ExecutionError> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct TestSource {
    running: Arc<AtomicBool>,
}

impl Source for TestSource {
    fn start(
        &self,
        fw: &mut dyn SourceChannelForwarder,
        _from_seq: Option<(u64, u64)>,
    ) -> Result<(), ExecutionError> {
        let operations = vec![
            (
                Operation::Insert {
                    new: Record::new(
                        None,
                        vec![Field::Int(0), Field::String("IT".to_string())],
                        None,
                    ),
                },
                DEPARTMENT_PORT,
            ),
            (
                Operation::Insert {
                    new: Record::new(
                        None,
                        vec![Field::Int(1), Field::String("HR".to_string())],
                        None,
                    ),
                },
                DEPARTMENT_PORT,
            ),
            (
                Operation::Insert {
                    new: Record::new(
                        None,
                        vec![
                            Field::Int(10000),
                            Field::String("Alice".to_string()),
                            Field::Int(0),
                            Field::Float(OrderedFloat(1.1)),
                        ],
                        None,
                    ),
                },
                USER_PORT,
            ),
            (
                Operation::Insert {
                    new: Record::new(
                        None,
                        vec![Field::Int(1), Field::String("HR".to_string())],
                        None,
                    ),
                },
                DEPARTMENT_PORT,
            ),
            (
                Operation::Insert {
                    new: Record::new(
                        None,
                        vec![
                            Field::Int(10001),
                            Field::String("Bob".to_string()),
                            Field::Int(0),
                            Field::Float(OrderedFloat(1.1)),
                        ],
                        None,
                    ),
                },
                USER_PORT,
            ),
            (
                Operation::Insert {
                    new: Record::new(
                        None,
                        vec![
                            Field::Int(10002),
                            Field::String("Craig".to_string()),
                            Field::Int(1),
                            Field::Float(OrderedFloat(1.1)),
                        ],
                        None,
                    ),
                },
                USER_PORT,
            ),
            (
                Operation::Insert {
                    new: Record::new(
                        None,
                        vec![
                            Field::Int(10003),
                            Field::String("Dan".to_string()),
                            Field::Int(0),
                            Field::Float(OrderedFloat(1.1)),
                        ],
                        None,
                    ),
                },
                USER_PORT,
            ),
            (
                Operation::Insert {
                    new: Record::new(
                        None,
                        vec![
                            Field::Int(10004),
                            Field::String("Eve".to_string()),
                            Field::Int(1),
                            Field::Float(OrderedFloat(1.1)),
                        ],
                        None,
                    ),
                },
                USER_PORT,
            ),
            (
                Operation::Delete {
                    old: Record::new(
                        None,
                        vec![
                            Field::Int(10002),
                            Field::String("Craig".to_string()),
                            Field::Int(1),
                            Field::Float(OrderedFloat(1.1)),
                        ],
                        None,
                    ),
                },
                USER_PORT,
            ),
            (
                Operation::Insert {
                    new: Record::new(
                        None,
                        vec![
                            Field::Int(10004),
                            Field::String("Frank".to_string()),
                            Field::Int(1),
                            Field::Float(OrderedFloat(1.5)),
                        ],
                        None,
                    ),
                },
                USER_PORT,
            ),
            (
                Operation::Update {
                    old: Record::new(
                        None,
                        vec![Field::Int(0), Field::String("IT".to_string())],
                        None,
                    ),
                    new: Record::new(
                        None,
                        vec![Field::Int(0), Field::String("XX".to_string())],
                        None,
                    ),
                },
                DEPARTMENT_PORT,
            ),
        ];

        for operation in operations.iter().enumerate() {
            match operation.1.clone().0 {
                Operation::Delete { old } => info!("{}: - {:?}", operation.1.clone().1, old.values),
                Operation::Insert { new } => info!("{}: + {:?}", operation.1.clone().1, new.values),
                Operation::Update { old, new } => {
                    info!(
                        "{}: - {:?}, + {:?}",
                        operation.1.clone().1,
                        old.values,
                        new.values
                    )
                }
            }
            fw.send(
                operation.0.try_into().unwrap(),
                0,
                operation.1.clone().0,
                operation.1.clone().1,
            )
            .unwrap();
        }

        loop {
            if !self.running.load(Ordering::Relaxed) {
                break;
            }
            thread::sleep(Duration::from_millis(500));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct TestSinkFactory {
    expected: u64,
    running: Arc<AtomicBool>,
}

impl TestSinkFactory {
    pub fn new(expected: u64, barrier: Arc<AtomicBool>) -> Self {
        Self {
            expected,
            running: barrier,
        }
    }
}

impl SinkFactory<SchemaSQLContext> for TestSinkFactory {
    fn get_input_ports(&self) -> Vec<PortHandle> {
        vec![DEFAULT_PORT_HANDLE]
    }

    fn set_input_schema(
        &self,
        _input_schemas: &HashMap<PortHandle, (Schema, SchemaSQLContext)>,
    ) -> Result<(), ExecutionError> {
        Ok(())
    }

    fn build(
        &self,
        _input_schemas: HashMap<PortHandle, Schema>,
    ) -> Result<Box<dyn Sink>, ExecutionError> {
        Ok(Box::new(TestSink {
            expected: self.expected,
            current: 0,
            running: self.running.clone(),
        }))
    }

    fn prepare(
        &self,
        _input_schemas: HashMap<PortHandle, (Schema, SchemaSQLContext)>,
    ) -> Result<(), ExecutionError> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct TestSink {
    expected: u64,
    current: u64,
    running: Arc<AtomicBool>,
}

impl Sink for TestSink {
    fn init(&mut self, _env: &mut LmdbEnvironmentManager) -> Result<(), ExecutionError> {
        debug!("SINK: Initialising TestSink");
        Ok(())
    }

    fn process(
        &mut self,
        _from_port: PortHandle,
        _op: Operation,
        _state: &SharedTransaction,
        _reader: &HashMap<PortHandle, Box<dyn RecordReader>>,
    ) -> Result<(), ExecutionError> {
        match _op {
            Operation::Delete { old } => info!("s: - {:?}", old.values),
            Operation::Insert { new } => info!("s: + {:?}", new.values),
            Operation::Update { old, new } => {
                info!("s: - {:?}, + {:?}", old.values, new.values)
            }
        }

        self.current += 1;
        if self.current == self.expected {
            debug!(
                "Received {} messages. Notifying sender to exit!",
                self.current
            );
            self.running.store(false, Ordering::Relaxed);
        }
        Ok(())
    }

    fn commit(&mut self, _epoch: &Epoch, _tx: &SharedTransaction) -> Result<(), ExecutionError> {
        Ok(())
    }
}

#[test]
#[ignore]
fn test_pipeline_builder() {
    dozer_tracing::init_telemetry(false).unwrap();

    let (mut pipeline, (node, port)) = statement_to_pipeline(
        "SELECT  department.name, SUM(user.salary) \
        FROM user JOIN department ON user.department_id = department.id \
        GROUP BY department.name",
    )
    .unwrap();

    let latch = Arc::new(AtomicBool::new(true));

    let mut asm = AppSourceManager::new();
    asm.add(AppSource::new(
        "conn1".to_string(),
        Arc::new(TestSourceFactory::new(latch.clone())),
        vec![
            ("user".to_string(), USER_PORT),
            ("department".to_string(), DEPARTMENT_PORT),
        ]
        .into_iter()
        .collect(),
    ))
    .unwrap();

    pipeline.add_sink(Arc::new(TestSinkFactory::new(8, latch)), "sink");
    pipeline
        .connect_nodes(&node, Some(port), "sink", Some(DEFAULT_PORT_HANDLE))
        .unwrap();

    let mut app = App::new(asm);
    app.add_pipeline(pipeline);

    let dag = app.get_dag().unwrap();

    let tmp_dir = TempDir::new("example").unwrap_or_else(|_e| panic!("Unable to create temp dir"));
    if tmp_dir.path().exists() {
        std::fs::remove_dir_all(tmp_dir.path())
            .unwrap_or_else(|_e| panic!("Unable to remove old dir"));
    }
    std::fs::create_dir(tmp_dir.path()).unwrap_or_else(|_e| panic!("Unable to create temp dir"));

    use std::time::Instant;
    let now = Instant::now();

    let tmp_dir = TempDir::new("test").unwrap();

    let mut executor = DagExecutor::new(
        &dag,
        tmp_dir.path(),
        ExecutorOptions::default(),
        Arc::new(AtomicBool::new(true)),
    )
    .unwrap();

    executor
        .start()
        .unwrap_or_else(|e| panic!("Unable to start the Executor: {e}"));
    assert!(executor.join().is_ok());

    let elapsed = now.elapsed();
    debug!("Elapsed: {:.2?}", elapsed);
}
