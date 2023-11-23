use std::time::Duration;
use fbs_amqp::*;
use fbs_runtime::{async_run, async_sleep};

#[test]
fn connect_test() {
    async_run(async {
        let mut amqp = AmqpConnection::new("asd".to_string());
        let result = amqp.connect("guest", "guest").await;
        assert!(result.is_err());
    });    
}


#[test]
fn good_connect_test() {
    async_run(async {
        let mut amqp = AmqpConnection::new("localhost".to_string());
        let result = amqp.connect("guest", "guest").await;
        dbg!(result);

        let mut channel = amqp.channel_open().await.unwrap();
        println!("Got channel opened!");

        let flow = channel.flow(true).await;
        println!("After flow!");
        dbg!(flow);

        let _ = channel.declare_exchange("test-exchange".to_string(), "direct".to_string(), AmqpExchangeFlags::new()).await;
        println!("After declare exchange!");

        let _ = channel.declare_queue("test-queue".to_string(), AmqpQueueFlags::new().durable(true)).await;
        println!("After declare queue!");

        let _ = channel.bind_queue("test-queue".to_string(), "test-exchange".to_string(), "test-key".to_string(), false).await;
        println!("After queue bind!");

        let _ = channel.purge_queue("test-queue".to_string(), false).await;
        println!("After queue purge!");

        let _ = channel.qos(0, 1, false).await;
        println!("After basic qos!");

        let tag = channel.consume("test-queue".to_string(), String::new(), Box::new(|_, _, _, _, _| { }), AmqpConsumeFlags::new()).await.unwrap();
        println!("After basic consume!");

        let _ = channel.cancel(tag, false).await;
        println!("After basic cancel!");

        let _ = channel.unbind_queue("test-queue".to_string(), "test-exchange".to_string(), "test-key".to_string()).await;
        println!("After queue unbind!");

        let _ = channel.delete_queue("test-queue".to_string(), AmqpDeleteQueueFlags::new()).await;
        println!("After queue delete!");

        let _ = channel.delete_exchange("test-exchange".to_string(), AmqpDeleteExchangeFlags::new().if_unused(true)).await;
        println!("After delete exchange!");

        let r2 = channel.close().await;
        dbg!(r2);
        println!("Got channel closed!");

        amqp.close().await;
        println!("Got connection closed!");
    });
}

#[test]
fn publish_test() {
    async_run(async {
        let mut amqp = AmqpConnection::new("localhost".to_string());
        let _ = amqp.connect("guest", "guest").await;

        let mut channel = amqp.channel_open().await.unwrap();
        println!("Got channel opened!");

        let _ = channel.flow(true).await;
        println!("After flow!");

        let _ = channel.declare_exchange("test-exchange".to_string(), "direct".to_string(), AmqpExchangeFlags::new()).await;
        println!("After declare exchange!");

        let _ = channel.declare_queue("test-queue".to_string(), AmqpQueueFlags::new().durable(true)).await;
        println!("After declare queue!");

        // let _ = channel.purge_queue("test-queue".to_string(), false).await;
        // println!("After queue purge!");

        let mut properties = AmqpBasicProperties::default();
        properties.content_type = Some("text/plain".to_string());
        properties.correlation_id = Some("correlation_id test".to_string());
        properties.app_id = Some("app_id test".to_string());
        properties.timestamp = Some(234524);
        properties.cluster_id = Some("cluster_id test".to_string());
        properties.message_id = Some("message id test".to_string());
        properties.priority = Some(2);

        channel.set_on_return(Some(Box::new(|code, reason, exchange, routing_key, message| {
            println!("Got return ({}, {}, {}, {}) - {:?}", code, reason, exchange, routing_key, message);
        })));

        let consume = Box::new(|tag, redelivered, exchange, routing_key, message| {
            println!("Got message ({}, {}, {}, {}) - {:?}", tag, redelivered, exchange, routing_key, message);
        });

        let tag = channel.consume("test-queue".to_string(), String::new(), consume, AmqpConsumeFlags::new()).await.unwrap();
        println!("After basic consume!");

        let _ = channel.publish("".to_string(), "test-queue".to_string(), properties, AmqpPublishFlags::new().mandatory(true), "test-data".as_bytes());

        channel.recover(true).await;
        async_sleep(Duration::new(2, 0)).await;

        let r2 = channel.close().await;
        dbg!(r2);
        println!("Got channel closed!");

        amqp.close().await;
        println!("Got connection closed!");
    });
}

#[test]
fn no_sense_test() {
    async_run(async {
        let mut amqp = AmqpConnection::new("localhost".to_string());
        let _ = amqp.connect("guest", "guest").await;

        let mut channel = amqp.channel_open().await.unwrap();
        println!("Got channel opened!");

        let _ = channel.declare_queue("test-queue-empty".to_string(), AmqpQueueFlags::new().durable(true)).await;
        println!("After declare queue!");

        let envelope = channel.get("test-queue".to_string(), false).await.unwrap();
        println!("After basic consume!");

        match envelope {
            None => (),
            Some((tag, _, _, _, _, _)) => {
                channel.nack(tag, AmqpNackFlags::new());
            },
        }

        dbg!(envelope);

        async_sleep(Duration::new(2, 0)).await;

        let r2 = channel.close().await;
        dbg!(r2);
        println!("Got channel closed!");

        amqp.close().await;
        println!("Got connection closed!");
    });
}    

#[test]
fn confirm_test() {
    async_run(async {
        let mut amqp = AmqpConnection::new("localhost".to_string());
        let _ = amqp.connect("guest", "guest").await;

        let mut channel = amqp.channel_open().await.unwrap();
        println!("Got channel opened!");

        let on_ack = Box::new(|tag, multiple| {
            println!("Got ack");
        });

        let on_nack = Box::new(|tag, flags| {
            println!("Got nack");
        });

        channel.confirm_select((on_ack, on_nack), false).await;

        let _ = channel.publish("".to_string(), "test-queue".to_string(), AmqpBasicProperties::default(), AmqpPublishFlags::new().mandatory(true), "test-data".as_bytes());

        async_sleep(Duration::new(2, 0)).await;

        let r2 = channel.close().await;
        dbg!(r2);
        println!("Got channel closed!");

        amqp.close().await;
        println!("Got connection closed!");
    });
}    
