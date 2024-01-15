use std::time::Duration;
use std::rc::Rc;
use std::cell::Cell;

use fbs_amqp::*;
use fbs_runtime::{async_run, async_sleep};

#[test]
fn bad_connect_test() {
    async_run(async {
        let params = AmqpConnectionParams::default();
        let connection = AmqpConnection::connect(params).await;

        assert!(connection.is_err());
    });
}

#[test]
fn good_connect_test() {
    async_run(async {
        let mut params = AmqpConnectionParams::default();
        params.address = "localhost".to_string();
        params.username = "guest".to_string();
        params.password = "guest".to_string();
        params.vhost = "/".to_string();

        let connection = AmqpConnection::connect(params).await;
        assert!(connection.is_ok());
    });
}
#[test]
fn basic_operations_test() {
    let result = async_run::<Result<(), AmqpConnectionError>>(async {
        let mut params = AmqpConnectionParams::default();
        params.address = "localhost".to_string();
        params.username = "guest".to_string();
        params.password = "guest".to_string();
        params.vhost = "/".to_string();

        let mut amqp = AmqpConnection::connect(params).await?;
        let mut channel = amqp.channel_open().await?;
        channel.declare_exchange("test-exchange-1".to_string(), "direct".to_string(), AmqpExchangeFlags::new()).await?;
        channel.declare_queue("test-queue-1".to_string(), AmqpQueueFlags::new().durable(true)).await?;
        channel.bind_queue("test-queue-1".to_string(), "test-exchange-1".to_string(), "test-key-1".to_string(), false).await?;
        channel.purge_queue("test-queue-1".to_string(), false).await?;
        channel.qos(0, 1, false).await?;
        let tag = channel.consume("test-queue-1".to_string(), String::new(), Box::new(|_, _, _, _, _| { }), AmqpConsumeFlags::new()).await?;
        channel.recover(true).await?;
        channel.cancel(tag, false).await?;
        channel.unbind_queue("test-queue-1".to_string(), "test-exchange-1".to_string(), "test-key-1".to_string()).await?;
        channel.delete_queue("test-queue-1".to_string(), AmqpDeleteQueueFlags::new()).await?;
        channel.delete_exchange("test-exchange-1".to_string(), AmqpDeleteExchangeFlags::new()).await?;
        channel.close().await?;
        amqp.close().await;
        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn consume_test() {
    let result = async_run::<Result<(), AmqpConnectionError>>(async {
        let mut params = AmqpConnectionParams::default();
        params.address = "localhost".to_string();
        params.username = "guest".to_string();
        params.password = "guest".to_string();
        params.vhost = "/".to_string();

        let mut amqp = AmqpConnection::connect(params).await?;
        let mut channel = amqp.channel_open().await?;
        let publisher = channel.publisher();

        let mut properties = AmqpBasicProperties::default();
        properties.content_type = Some("text/plain".to_string());
        properties.correlation_id = Some("correlation_id test".to_string());
        properties.app_id = Some("app_id test".to_string());
        properties.timestamp = Some(234524);
        properties.cluster_id = Some("cluster_id test".to_string());
        properties.message_id = Some("message id test".to_string());
        properties.priority = Some(2);

        let counter = Rc::new(Cell::new(0));
        let counter_copy = counter.clone();

        let consume = Box::new(move |_, _, exchange, routing_key, message: &mut AmqpMessage| {
            assert_eq!(exchange, "");
            assert_eq!(routing_key, "test-queue-2");
            assert_eq!(message.properties.content_type, Some("text/plain".to_string()));
            assert_eq!(message.properties.correlation_id, Some("correlation_id test".to_string()));
            assert_eq!(message.properties.app_id, Some("app_id test".to_string()));
            assert_eq!(message.properties.timestamp, Some(234524));
            assert_eq!(message.properties.cluster_id, Some("cluster_id test".to_string()));
            assert_eq!(message.properties.message_id, Some("message id test".to_string()));
            assert_eq!(message.properties.priority, Some(2));
            assert_eq!(message.content.as_slice(), "test-content".as_bytes());
            counter_copy.set(counter_copy.get() + 1);
        });

        channel.declare_queue("test-queue-2".to_string(), AmqpQueueFlags::new().durable(true)).await?;
        channel.purge_queue("test-queue-2".to_string(), false).await?;
        channel.consume("test-queue-2".to_string(), String::new(), consume, AmqpConsumeFlags::new()).await?;

        publisher.publish("".to_string(), "test-queue-2".to_string(), properties, AmqpPublishFlags::new(), "test-content".as_bytes())?;

        async_sleep(Duration::new(1, 0)).await;
        channel.delete_queue("test-queue-2".to_string(), AmqpDeleteQueueFlags::new()).await?;
        channel.close().await?;

        println!("buffer-stats: {:?}", amqp.get_buffer_stats());
        amqp.close().await;

        assert_eq!(counter.get(), 1);
        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn return_test() {
    let result = async_run::<Result<(), AmqpConnectionError>>(async {
        let mut params = AmqpConnectionParams::default();
        params.address = "localhost".to_string();
        params.username = "guest".to_string();
        params.password = "guest".to_string();
        params.vhost = "/".to_string();

        let mut amqp = AmqpConnection::connect(params).await?;
        let mut channel = amqp.channel_open().await?;
        let publisher = channel.publisher();

        let mut properties = AmqpBasicProperties::default();
        properties.content_type = Some("text/plain".to_string());
        properties.correlation_id = Some("correlation_id test".to_string());
        properties.app_id = Some("app_id test".to_string());
        properties.timestamp = Some(234524);
        properties.cluster_id = Some("cluster_id test".to_string());
        properties.message_id = Some("message id test".to_string());
        properties.priority = Some(2);

        publisher.publish("".to_string(), "test-queue-nonexisting".to_string(), properties, AmqpPublishFlags::new().mandatory(true), "test-content".as_bytes())?;

        let counter = Rc::new(Cell::new(0));
        let counter_copy = counter.clone();

        let return_cb = Box::new(move |_, _, exchange, routing_key, message: &mut AmqpMessage| {
            assert_eq!(exchange, "");
            assert_eq!(routing_key, "test-queue-nonexisting");
            assert_eq!(message.properties.content_type, Some("text/plain".to_string()));
            assert_eq!(message.properties.correlation_id, Some("correlation_id test".to_string()));
            assert_eq!(message.properties.app_id, Some("app_id test".to_string()));
            assert_eq!(message.properties.timestamp, Some(234524));
            assert_eq!(message.properties.cluster_id, Some("cluster_id test".to_string()));
            assert_eq!(message.properties.message_id, Some("message id test".to_string()));
            assert_eq!(message.properties.priority, Some(2));
            assert_eq!(message.content.as_slice(), "test-content".as_bytes());
            counter_copy.set(counter_copy.get() + 1);
        });

        channel.set_on_return(Some(Box::new(return_cb)));


        async_sleep(Duration::new(1, 0)).await;
        channel.close().await?;

        println!("buffer-stats: {:?}", amqp.get_buffer_stats());
        amqp.close().await;

        assert_eq!(counter.get(), 1);
        Ok(())
    });

    assert!(result.is_ok());
}


#[test]
fn get_test() {
    let result = async_run::<Result<(), AmqpConnectionError>>(async {
        let mut params = AmqpConnectionParams::default();
        params.address = "localhost".to_string();
        params.username = "guest".to_string();
        params.password = "guest".to_string();
        params.vhost = "/".to_string();

        let mut amqp = AmqpConnection::connect(params).await?;
        let mut channel = amqp.channel_open().await?;
        let publisher = channel.publisher();

        let mut properties = AmqpBasicProperties::default();
        properties.content_type = Some("text/plain".to_string());
        properties.correlation_id = Some("correlation_id test".to_string());
        properties.app_id = Some("app_id test".to_string());
        properties.timestamp = Some(234524);
        properties.cluster_id = Some("cluster_id test".to_string());
        properties.message_id = Some("message id test".to_string());
        properties.priority = Some(2);

        channel.declare_queue("test-queue-3".to_string(), AmqpQueueFlags::new().durable(true)).await?;
        channel.purge_queue("test-queue-3".to_string(), false).await?;

        publisher.publish("".to_string(), "test-queue-3".to_string(), properties, AmqpPublishFlags::new(), "test-content".as_bytes())?;
        async_sleep(Duration::new(1, 0)).await;

        let result = channel.get("test-queue-3".to_string(), true).await?;
        assert!(result.is_some());
        match result {
            None => panic!(),
            Some((_, _, exchange, routing_key, _, message)) => {
                assert_eq!(exchange, "");
                assert_eq!(routing_key, "test-queue-3");
                assert_eq!(message.properties.content_type, Some("text/plain".to_string()));
                assert_eq!(message.properties.correlation_id, Some("correlation_id test".to_string()));
                assert_eq!(message.properties.app_id, Some("app_id test".to_string()));
                assert_eq!(message.properties.timestamp, Some(234524));
                assert_eq!(message.properties.cluster_id, Some("cluster_id test".to_string()));
                assert_eq!(message.properties.message_id, Some("message id test".to_string()));
                assert_eq!(message.properties.priority, Some(2));
                assert_eq!(message.content.as_slice(), "test-content".as_bytes());
            },
        }

        let result = channel.get("test-queue-3".to_string(), true).await?;
        assert!(result.is_none());

        channel.delete_queue("test-queue-3".to_string(), AmqpDeleteQueueFlags::new()).await?;

        channel.close().await?;

        println!("buffer-stats: {:?}", amqp.get_buffer_stats());
        amqp.close().await;

        Ok(())
    });

    assert!(result.is_ok());
}
