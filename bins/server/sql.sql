create table tracing_record
(
--     id                   bigint      not null,
    app_run_record_index bigint      not null,
    app_id               uuid        not null,
    app_version          varchar(16) not null,
    app_run_id           uuid        not null,
    node_id              varchar(64) not null,
    name                 text        not null,
    record_time          timestamptz not null,
    kind                 varchar(16) not null,
    creation_time        timestamptz not null default now(),
    level                int,
    span_id              uuid,
    parent_span_t_id     bigint,
    parent_id            uuid,
    fields               jsonb,
    target               varchar(64),
    module_path          varchar(128),
    position_info        varchar(128) -- file;line
);

create table app
(
    id            uuid primary key,
    creation_time timestamptz not null default now()
);
create table app_build
(
    app_id        uuid         not null,
    app_version   varchar(128) not null,
    app_name      varchar(128) not null,
    creation_time timestamptz  not null default now(),
    primary key (app_id, app_version),
    foreign key (app_id) references app
);
create table app_run
(
    id             uuid primary key,
    app_id         uuid         not null,
    app_version    varchar(128) not null,
    node_id        varchar(64)  not null,
    data           jsonb,
    creation_time  timestamptz  not null default now(),
    record_id      bigint       not null,
    stop_record_id bigint,
    start_time     timestamptz  not null,
    stop_time      timestamptz  null,
    exception_end  bool,
    foreign key (app_id, app_version) references app_build
);

-- create table node


create table tracing_span
(
    id            uuid primary key, -- record_time
    position_info varchar(128) not null,
    name          text         not null,
    app_id        uuid         not null,
    app_version   varchar(128) not null
);


create table tracing_span_parent
(
    span_id        uuid not null,
    span_parent_id uuid,
    primary key (span_id, span_parent_id),
    foreign key (span_id) references tracing_span,
    foreign key (span_parent_id) references tracing_span
);

create table tracing_span_run
(
    id              uuid primary key,
    app_run_id      uuid        not null,
    span_id         uuid        not null,
    run_time        timestamptz not null,
    busy_duration   double precision,
    idle_duration   double precision,
    record_id       bigint      not null,
    close_record_id bigint      null,
    exception_end   timestamptz,
    fields          jsonb,
    foreign key (app_run_id) references app_run,
    foreign key (span_id) references tracing_span
);

create table tracing_span_enter
(
    id              uuid primary key,
    span_run_id     uuid        not null,
    enter_time      timestamptz not null,
    duration        double precision,
    record_id       bigint      not null,
    leave_record_id bigint      null,
    foreign key (span_run_id) references tracing_span_run
);

create index tracing_record_fields on tracing_record using gin (fields);

-- create unique index tracing_record_unique_id on tracing_record (id, record_time);
create index tracing_record_id on tracing_record (id);
create index tracing_record_app_id on tracing_record (app_id);
-- create index tracing_record_app_build_id on tracing_record (app_version);
create index tracing_record_app_run_id on tracing_record (app_run_id);
create index tracing_record_name on tracing_record using hash (name);
create index tracing_record_node_id on tracing_record (node_id);
create index tracing_record_kind on tracing_record (kind);
create index tracing_record_level on tracing_record (level);
create index tracing_record_parent_span_t_id on tracing_record (parent_span_t_id);

create index tracing_span_id_name on tracing_span (position_info);
create index tracing_span_id_position_info on tracing_span (name);

create table setting
(
    id     serial primary key,
    name   varchar(128) not null,
    value  json         not null,
    app_id uuid
);

SELECT create_hypertable('tracing_record', 'record_time');
-- ALTER SYSTEM SET default_toast_compression = 'lz4';


-- ALTER TABLE tracing_record
--     SET (
--         timescaledb.compress,
--         timescaledb.compress_segmentby = 'kind,span_id',
--         timescaledb.compress_orderby = 'id asc'
--         );

-- timescaledb.compress_orderby = 'id asc'

-- SELECT set_chunk_time_interval('tracing_record', INTERVAL '24 hours');

-- SELECT add_compression_policy('tracing_record', INTERVAL '24 days');
-- SELECT remove_compression_policy('tracing_record');
--
-- SELECT * FROM show_chunks('tracing_record');
--
-- --
-- -- SELECT compress_chunk('_timescaledb_internal._hyper_34_33_chunk');
-- --
-- SELECT pg_size_pretty(before_compression_total_bytes) as before_compression_total_bytes,
--        pg_size_pretty(after_compression_total_bytes)  as after_compression_total_bytes,
--        pg_size_pretty(before_compression_index_bytes) as before_compression_index_bytes,
--        pg_size_pretty(after_compression_index_bytes)  as after_compression_index_bytes,
--        pg_size_pretty(before_compression_table_bytes) as before_compression_table_bytes,
--        pg_size_pretty(after_compression_table_bytes)  as after_compression_table_bytes
--
-- FROM hypertable_compression_stats('tracing_record');
--
--
-- SELECT * FROM timescaledb_information.jobs;
--
-- SELECT show_chunks('tracing_record');

-- SET timescaledb.enable_compression_indexscan = 'ON';
