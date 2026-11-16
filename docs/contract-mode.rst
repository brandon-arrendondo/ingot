Contract Mode
=============

Contract mode (``ingot --contract <contract.toml>``) turns a keyed data model into the tables a
key-value store core reads, a machine-readable contract, a keyspace declaration for tooling, and,
when the model maps keys to a device shadow, the tables a shadow binder, reporter and command
router walk. Every rule below is checked at generation time: a model that breaks any of them
generates nothing, and every refusal is logged with the offending item. ``examples/contract/``
holds a generic model at contract 1.0, 1.1 and 2.0.

Inputs
------

``contract.toml``
   The contract (``[contract]``: ``name``, ``major``, ``minor``, ``patch``, ``namespace_dir``,
   optional ``variants`` and ``units``), the queue, staging and log ``[bounds]``,
   ``[[reserved]]`` IDs, ``[[principals]]`` with their ``grants`` and ``log`` stream,
   ``[[uid_caps]]``, ``[[error_groups]]``, and the contract-level shadow rows: ``[[apply_groups]]``, ``[[shadow_echo]]``,
   ``[[shadow_constant]]``, ``[shadow_rejected]``, ``[[topics]]``, ``[[topic_payload]]`` and
   ``[[placeholders]]``.

Namespace files (every ``.toml`` in ``namespace_dir``)
   ``[meta]`` (``id``, ``namespace_id``, ``mfg``), ``[enums.<name>]``, ``[[classes]]`` with their
   ``[[classes.keys]]`` and ``[[classes.commands]]``, and the namespace's ``[[shadow_commands]]``.

``units.toml`` (when ``units`` names it)
   The unit vocabulary: token, quantity and meaning.

Every input file enters the model hash. ``--variant <name>`` selects one of the declared
variants; when variants are declared it is required.

Keys, classes and commands
--------------------------

A key's ID encodes its namespace, class, index and type. A class has a role (``settings``,
``state``, ``command``, ``config``, ``diag``) and a default persistence (``none``, ``deferred``,
``durable``). A key declares its ``type``, ``introduced_in`` minor, and optionally ``write_rate``,
``rate_burst``, ``persistence``, ``deferred_ms``, ``secret`` with its ``readers``,
``survives_reset``, ``alias_of``, ``default`` (``defaults`` per variant), ``enum``, ``max_size``,
``wk`` (a well-known role), ``doc``, ``variants`` (the variants it exists on), and an ``error``
bit. A ``write_rate`` (``"<n>/s|min|h|day"``) becomes ``rate_per_day`` and a token-bucket
``rate_burst``: the declared one, else ``[bounds] default_rate_burst``; a ``rate_burst`` without a
``write_rate`` is refused, and so is a declared ``rate_burst`` of 0, which would switch the
store's rate limit off (a key or command that wants no limit omits ``write_rate``); for the same
reason ``[bounds] default_rate_burst`` must be at least 1, whether or not any declaration uses
it. A command type (``[[classes.commands]]``) creates its command, result
and dedupe keys with its owner, payload kind and sizes; ``internal`` says why it is never
requested on the wire. Its ``write_rate`` and ``rate_burst`` apply to all three keys alike (each
row of ``dmt_keys`` and each key of ``contract.json`` carries them), under the same rules:

.. code-block:: toml

   [[classes.commands]]
   id = "map_edit"
   # ... indices, owner, sizes ...
   write_rate = "1/min"   # rate_per_day 1440 on map_edit, map_edit_result and map_edit_dedupe
   rate_burst = 8         # rate_burst 8 on all three; absent: the default burst

``outcome = "<enum>"`` names an enum of the command's namespace whose values are the codes its
result's outcome field carries (``DMT_RESULT_HEADER_BYTES``: epoch, sequence, outcome); it changes
no table, only ``dmt_constants.h`` and ``contract.json``:

.. code-block:: toml

   [enums.drive_outcome.values]
   accepted = 0
   refused = 1
   busy = 2

   [[classes.commands]]
   id = "drive"
   # ... indices, owner, sizes ...
   outcome = "drive_outcome"   # DMT_E_DRIVE_COMMAND_DRIVE_ACCEPTED 0, ..._REFUSED 1, ..._BUSY 2

Numeric keys may carry a ``unit`` (a token of the vocabulary), an inclusive ``min``/``max`` with
``step``, or a discrete ``allowed`` set, a ``scale``, and an ``unset`` sentinel default outside the
range.

A ``struct`` key may declare the fixed ``header`` its value starts with: an ordered list of
fields, each ``{ name, type }`` with a C identifier unique within the key and a type of ``bool``,
``uint8``, ``int8``, ``uint16``, ``int16``, ``uint32`` or ``int32``. The value is the fields in
declaration order, each little-endian and packed with no padding (a ``bool`` is one byte, 0 or
1), then the payload: the rest of the value, possibly empty. The store stays layout-blind: a
header changes no store table and no KSET digest, only ``contract.json`` and ``dmt_constants.h``
(and the SDK binding context):

.. code-block:: toml

   [[classes.keys]]
   id = "map_data"
   key_index = 0
   type = "struct"
   max_size = 65536
   introduced_in = "1.0"
   header = [{ name = "session_id", type = "uint32" },   # offset 0
             { name = "map_id", type = "int32" },        # offset 4
             { name = "global", type = "bool" }]         # offset 8; 9 header bytes

Enums
-----

``[enums.<name>.values]`` maps a member name to its code, or to ``{ value, wire, accept }``:
``wire`` is the string the member is reported as, ``accept`` lists further inbound strings. A
member may instead declare ``report_as = "<member or wire string>"`` to report another member's
string (many codes to one string outbound). ``[enums.<name>.variants.<v>]`` removes members and
adds or renames values for one variant. Within a resolved enum every code, name and inbound
string is unique.

Principals and rights
---------------------

A principal has a name, ``uid``, ``token`` (0 for the uid itself), ``kind`` (``unix``,
``inproc``, ``uart``), flags, heartbeat, budgets and caps, ``introduced_in`` and ``grants``
(a namespace, class or key with ``r``, ``w`` or ``rw``; the last grant on a key wins). The rights
table, per-principal handshake digests (KSET v1, one per contract minor) and queue bounds follow.
``dmt_principal_names`` holds every principal's declared name, in table order, never ``NULL``.

Log streams
-----------

A principal may declare one log stream, as an inline table ``log = { ... }`` or as a
``[principals.log]`` table after the principal's other fields. The stream is runtime
configuration of the store's log service: it enters no KSET digest, no keyspace file and no
``--previous-contract`` check, so a minor or a patch may change it freely.

.. code-block:: toml

   [principals.log]
   name = "integrator"                   # absent: the principal's name, dots as underscores
   backing = "ring"                      # at boot: off (default), ring, flash_on_trigger, flash_group
   allowed_backings = ["ring", "flash_on_trigger"]      # absent: the backing; off is always allowed
   allowed_backings_by_variant = { sku_b = ["ring"] }   # backing_by_variant likewise
   ram_cap_bytes = 8192                  # RAM while enabled with any backing but off
   max_line_bytes = 256                  # a longer line is dropped whole and counted, never cut
   default_level_ram = ["error", "warning", "info"]     # error, warning, info, debug
   default_level_file = ["error", "warning"]            # within default_level_ram
   flash_seg_bytes = 16384               # one flash segment; with flash_seg_count segments
   flash_seg_count = 4
   flash_rate_bytes_per_day = 262144     # flash token bucket, in bytes
   flash_rate_burst_bytes = 16384
   flush = ["fault", "watchdog", "shutdown", "error_line"]
   ctl_key = "core.log.integrator_cfg"   # the runtime switch; some principal must write it
   drop_counter_key = "core.diag.log_integrator_drops"  # an unsigned integer key
   introduced_in = "1.1"

The flush triggers are ``fault``, ``watchdog``, ``shutdown``, ``reboot``, ``error_line``,
``request``, ``restart_storm`` and ``high_water``. ``[bounds]`` bounds the streams from above, on
each variant's run:

``log_ram_budget_bytes``
   The worst case with every stream enabled at once: the sum of ``ram_cap_bytes`` over the
   streams the variant allows a backing other than ``off``. Required with any stream.
``log_ram_default_budget_bytes`` (optional)
   The same sum over the streams whose boot backing on the variant is not ``off``.
``log_flash_bytes``, ``log_flash_bytes_per_day``
   Over the streams the variant allows a flash backing: the sum of ``flash_seg_bytes`` times
   ``flash_seg_count``, and the sum of ``flash_rate_bytes_per_day``. Required when any stream
   may write flash.

Ingot checks the structure and these sums only. Erase-block alignment and the wear figures of a
target are the model's own checks; ingot fixes none.

Error groups
------------

``[[error_groups]]`` declare a word of error bits: ``width``, ``wire_base`` (per variant in
``wire_bases``), ``writer``, ``reserved_bits``, ``publish_max_bit`` and the shadow path of the
error list. A bool STATE key with ``error = { group, bit }`` is one error; its wire id is
``e<wire_base + bit>``.

Shadow mapping
--------------

Paths are dotted identifiers: each dot is one nested JSON object.

Key paths
   ``shadow`` on a SETTINGS key is a desired path; on a STATE or CONFIG key it is a reported
   path. It may be a list, and each entry may be ``{ path, as, scope, variants }``: ``as =
   "integer"`` carries an enum code as a number (inbound and outbound), or a bool as ``0``/``1`` on
   a reported path (inbound a bool stays ``true``/``false``), ``scope`` (``full``, ``delta``,
   ``both``) places a reported path in the full document, the on-change delta or both,
   ``variants`` limits the entry. One path may be listed once per variant and document, so a key
   may report one path as ``0``/``1`` in the full document and as ``true``/``false`` in the delta
   (``DSH_FORM_BOOL_INTEGER``, shadow table interface 1.2). Further key attributes: ``applies`` (the SETTINGS key
   a STATE twin reports), ``apply_group`` and ``implies`` (coupled writes in one batch),
   ``on_reject`` (``echo`` or ``clear_desired``), ``inbound_unknown`` and ``outbound_unknown`` (what
   an unknown string or code becomes: a wire string, ``ignore``/``reject`` inbound, ``raw``
   outbound), ``forced`` (re-emitted on every delta), ``render`` (an index list reported as wire
   strings) and ``telem`` (a telemetry name).

``[[shadow_echo]]``
   A desired path reported back as received, with no key: ``path``, ``type``, ``max_size``,
   ``command_key`` (the SETTINGS key a command body applies it to), ``variants`` and ``scope``.
   One echo row may share its delta member with a reported key of the same JSON value.

``[[shadow_constant]]``
   A reported path with a literal ``value`` or an ``image`` value (the contract version, or the
   hash of the resolved keyspace body), optionally ``forced``.

``[shadow_rejected]``
   ``path``: the reported path where the binder lists the desired members it rejected in the
   update it answers. It is a delta row (``DSH_RK_REJECTED``, shadow table interface 1.3) with no
   key: the binder builds the list from the rows it rejected. ``variants`` limits it to those
   variants (a variant not listed reports no list). Absent, no list is reported. Its
   path is checked against the other delta rows, a model with no desired path may not declare
   it, and it is kept unchanged within a major.

``[[topics]]``
   ``id``, ``template`` over ``{stage}``, ``{duid}``, ``{model}``, ``{thing}`` and
   ``{shadow_name}``, ``direction`` (``pub`` or ``sub``), ``connection``, ``subscribe_order``,
   ``local_wrapper``, ``ack`` (the response topic), ``envelope`` (the request ``id`` field, required
   or with an absent value, and the ``source`` field with its enum and absent member),
   ``settings_in_every_body`` and ``literal_shadow_name``. A topic may declare ``vars = [{ name,
   max_len }]``: per-message placeholders, each ``{name}`` in the template, at most 8 per topic
   and each up to 128 bytes (``dtp_vars``, shadow table interface 1.4). A pub topic's publisher
   fills them; on a sub topic (interface 1.6) each var is one whole topic level, used once, which
   the consumer subscribes to as a single-level wildcard and extracts from a received topic. A
   template placeholder that is neither per-device nor declared is refused, and so is a declared
   var the template does not use. A var whose value is a code of a model enum names it, ``vars =
   [{ name, max_len, enum = "<namespace>::<enum>" }]`` (the form an envelope ``source`` names an
   enum with): the var carries the code as decimal text, and ``dmt_constants.h`` defines the
   enum's codes. It changes no table. Refused: an enum that is not defined, a negative code, and
   a code whose decimal text is longer than ``max_len``, each as the variant resolves the enum
   (a variant that adds a longer code is refused there). ``variants`` limits a topic to those variants: a
   variant not listed has no row, and a topic is compared on a previous contract's variant (a
   minor may add variants, not drop or change the topic there).

``[[placeholders]]``
   ``name`` and ``max_len``: a named value the consumer is configured with (a build or device
   value), usable as ``{name}`` in any topic template; at most 8, each up to 128 bytes, each used
   by some topic, and no topic var may take its name (``dtp_placeholders``, shadow table interface
   1.7); a placeholder may not name an ``enum`` (only a topic var carries a code). They are kept
   unchanged within a major.

``[[topic_payload]]``
   One member of a pub topic's payload: ``topic``, ``path``, and one of ``key`` (any key with a
   JSON value), ``value`` or ``image``; an enum member takes ``as`` or declares its
   ``outbound_unknown``; ``variants`` limits it.

``[[shadow_commands]]`` (in namespace files)
   How a request reaches the store: ``topic``, ``selector`` (``field`` equal to a ``wire`` string or
   flag, an array ``element``, or an ``object``), ``body`` (``fields``, ``settings`` or ``{ object,
   selector }``), ``protocol`` (``single`` or ``command_array``), and then either a ``command`` with
   ``args`` and further commands in ``also``, an ``answer`` code, an ``internal`` reason (with an
   optional ``internal_id``, 1 to 65535: the model's stable id of the action, which the library
   dispatches on instead of the wire name; ``dsh_command_t.internal_id``, shadow table interface
   1.5, and named in ``dsh_tables.h`` from interface 1.10), or args that write STATE keys only. An argument takes a request ``field`` (optionally an ``element`` and
   an ``encoding``: ``base64`` into a string, binary or struct key, ``decimal_text`` into an integer
   key) or a constant ``value``, with ``required`` and its ``ec``, an ``absent`` value, or
   ``omit_when_absent``. ``variants`` with ``grounded_in`` limits a row.

Outputs
-------

``dmt_types.h``
   The fixed table interface (TIV) of the store tables. TIV 1.1 appends the log stream codes
   (``DMT_LB_*`` backings, ``DMT_LL_*`` levels, ``DMT_LT_*`` flush triggers) and
   ``dmt_log_stream_t``.

``dmt_tables.h``, ``dmt_tables.c``
   Keys, defaults, reserved IDs, principals, rights, caps, commands, well-known roles and
   digests of the selected variant; and ``dmt_log_streams`` with ``dmt_log_stream_names``
   (``DMT_NUM_LOG_STREAMS`` rows in principal table order; one zero row, and ``NULL``, when there
   are none), each row with the variant's boot backing, its allowed-backing mask (``1u <<
   DMT_LB_*`` each, ``off`` always set), the level masks, the flush mask, the key IDs of its
   switch and drop counter and its principal's index, and the ``[bounds]`` log budgets as
   ``DMT_LOG_RAM_BUDGET_BYTES``, ``DMT_LOG_RAM_DEFAULT_BUDGET_BYTES``, ``DMT_LOG_FLASH_BYTES`` and
   ``DMT_LOG_FLASH_BYTES_PER_DAY`` (0 when undeclared), for the store to re-check at startup.

``dmt_constants.h``
   The code of every value of every enum key as ``#define DMT_E_<KEY PATH>_<VALUE> <code>`` for the
   selected variant (upper case, dots as underscores), so no consumer writes an enum code by hand,
   and of every command's ``outcome`` enum as ``#define DMT_E_<COMMAND PATH>_<VALUE> <code>``,
   placed at the command key's ID. It is per model, outside the table interface; a name that is
   not a C identifier, is longer than 63 characters, or is produced by two (key or command, value)
   pairs is refused. Constants are kept within a major, as enum values and outcomes are.

   After those, every enum a topic var of the variant names (``enum = "<namespace>::<enum>"``) is
   defined once, named after the enum rather than the var (several vars may name one enum):
   ``#define DMT_E_<NAMESPACE>_<ENUM>_<VALUE> <code>``, enums in reference order and values in
   code order, as the variant resolves the enum. The same name checks apply (an enum constant
   that is also a key's is refused), and the file's header comment says so only when some var
   names an enum. For example ``vars = [{ name = "event", max_len = 1, enum = "map::event" }]``
   with ``[enums.event.values] update = 1`` in namespace ``map`` gives
   ``#define DMT_E_MAP_EVENT_UPDATE 1``.

   For every key of the variant with a ``header`` it also holds (``<KEY>`` the key path in upper
   case, ``<key>`` in lower case, dots as underscores): ``DMT_<KEY>_HEADER_BYTES``,
   ``DMT_<KEY>_<FIELD>_OFFSET`` per field and ``DMT_<KEY>_PAYLOAD_MAX`` (``max_size`` minus the
   header bytes); ``dmt_<key>_header_t``, a plain struct of the fields' native C types (not the
   wire image); and ``static inline void dmt_<key>_header_encode(const dmt_<key>_header_t *h,
   uint8_t *out)``, which writes the header's bytes, and ``static inline int
   dmt_<key>_header_decode(const uint8_t *in, uint32_t len, dmt_<key>_header_t *h)``, which
   returns 0, or -1 with ``h`` untouched when ``len`` is below the header bytes or a bool byte is
   neither 0 nor 1. Both work by explicit byte shifts, never a packed struct, so neither depends
   on the host's byte order or alignment. A writer encodes the header and appends the payload in
   one buffer; a reader decodes it and takes the payload at ``value + DMT_<KEY>_HEADER_BYTES``.
   A header name that is longer than 63 characters, or that another header or enum constant
   also produces, is refused. Without a header the file is as before.

``contract.json``
   Every key, principal, right, command (with its ``outcome`` names and codes, when it has any)
   and shadow row, read back by ``--previous-contract``. A key with a header carries ``header``
   (``[{name, type, offset, size}]``) and ``header_bytes``; a key without one has neither. A topic
   var that names an enum carries it as ``shadow.topics[].vars[].enum`` (and so does the
   keyspace, whose body hash ``DSH_CONTRACT_HASH`` then moves), and ``var_enums`` maps each such
   ``<namespace>::<enum>`` to its names and codes; without one, neither appears.
   ``log_streams`` lists each log stream as the variant resolves it, for tooling to read; it is
   never compared, and absent without a stream.

``keyspace.json``, ``keyspace.<variant>.json`` (with ``.json.gz`` and ``.head.json``), ``keyspace.schema.json``
   The declaration for tooling: all variants with per-variant overrides, the resolved variant,
   its deterministic gzip body and head (size and SHA-256), and the JSON schema. No values and no
   secret values appear.

``dsh_types.h``, ``dsh_tables.h``, ``dsh_tables.c`` (when the model maps the shadow)
   The fixed shadow table interface and the variant's binder (``dsh_desired``, ``dsh_echo``,
   enums with inbound and outbound strings, coupled writes, discrete sets), reporter
   (``dsh_reported`` in emission order), error-group, command-router (``dsh_commands``,
   ``dsh_steps``, ``dsh_args``) and topic (``dtp_topics``, ``dtp_payload``) tables, the contract
   hash, and the worst-case full and forced-delta document sizes. Shadow table interface 1.8 adds
   the bounds a consumer sizes its buffers with: ``DSH_REJECTED_MAX_ITEMS`` (one member per
   desired and echo path; 0 without a rejected-settings list), ``DSH_DESIRED_PATH_MAX_BYTES`` (the
   longest desired or echo path with its terminator), ``DSH_ERROR_IDS_PUBLISHED`` (every group's
   bits up to ``publish_max_bit`` that are not reserved) and ``DSH_ERROR_ID_MAX`` (the largest wire
   id of a published or declared bit). Interface 1.9 adds ``DSH_DESIRED_VALUE_MAX_BYTES``: 1 plus
   the longest JSON text of a value any ``dsh_desired`` or ``dsh_echo`` row accepts (0 with no
   row), each counted at the longest text the document size bound uses for its type: a bool 5, an
   integer type its longer extreme in decimal, a float32 47 (``%f`` of ``-FLT_MAX``), and a JSON
   string that unescapes to *n* bytes 2 + 6\ *n* (every byte as ``\u00XX``). A string row accepts
   up to ``max_size`` bytes; an enum wire-string row accepts its wire strings and accept aliases
   (an unknown string its ``inbound_unknown`` rule maps is not counted); an enum integer row and
   an echo row count by their type (an ``int`` echo as int32). A desired value the binder
   refuses can be any text the sender wrote, so a consumer listing refused values leaves out one
   longer than ``DSH_DESIRED_VALUE_MAX_BYTES - 1``. Interface 1.10 adds
   ``#define DSH_INTERNAL_<VERB> <internal_id>u`` for every internal ``shadow_commands`` row of the
   variant that declares an ``internal_id``, so the library dispatches on a name and never repeats
   the number. ``<VERB>`` is the text that selects the row, from its innermost selector (the body
   object's selector for an object body): the ``wire`` string, else the ``element``, else the
   ``field``, in upper case with dots as underscores (``state=getlog`` gives
   ``DSH_INTERNAL_GETLOG``, ``cmd[stopPolling]`` ``DSH_INTERNAL_STOPPOLLING``, ``net.reset=true``
   ``DSH_INTERNAL_NET_RESET``). Rows that share a name and an id give one constant, in row order;
   a row on some variants gives its constant on those variants only. A name that is not a C
   identifier or is longer than 63 characters, or one name for two ids on a shared variant, is
   refused. A variant without such a row states none.

``sdk/<principal>/`` (with ``--sdk-binding <DIR>``)
   Each file of the binding directory rendered once per principal, named after the file without a
   trailing ``.tera``; see the CLI reference for the naming rule and the context. Ingot names no
   store runtime. Besides the accessors and submit helpers, the context lists an event view for
   each struct key the principal may read: a struct value has no getter and reaches a reader only
   as a delivered event, and the view tells the reader which key an event carries and gives its
   whole value (a declared header included), so no consumer writes a key ID by hand.

``release-review.txt`` (with ``--previous-contract``)
   New principals' sensitive grants, for the release review.

Versions
--------

Within a major a contract only grows: keys, principals and rights are added at a new minor,
existing keys keep their ID, persistence, limits (ranges and sets may widen), enum values and
well-known roles and struct value header (a header added, removed, reordered, renamed or
retyped is a major, as a type change is), a command keeps its outcomes (it may gain outcomes, or
name them for the first time), error bits stay, and the shadow mapping only grows: a mapped key keeps its path
entries and wire attributes, its paths and accept aliases may be added to, and every echo,
constant, topic, command and payload row stays as it was. A topic var may name an enum for the
first time in a minor; one it named stays named, with every name and code of that enum kept. A major reserves the IDs of removed keys.
Write rates and bursts are not frozen: a minor may change the ``write_rate`` or ``rate_burst`` of
a released key or command. That is a rate-table change (``rate_per_day`` and ``rate_burst`` in
``dmt_keys`` and ``contract.json``): no KSET digest and no release-review line depends on either;
the keyspace files carry the rate (so a rate change moves their body hash) but not the burst.
``--previous-contract`` checks all of this against the previous release's ``contract.json``.

Refusals
--------

Every rule above has a refusal, grouped as follows:

* **Layout and IDs:** duplicate namespace IDs, names or encoded IDs, a reserved or zero ID, a
  key index out of range, a type that cannot hold its default or size.
* **Values:** a default outside its type, enum, range or set; an unknown unit; a scale without a
  unit; a range or set on a non-numeric or enum key; a key ``enum`` or command ``outcome`` naming
  no enum of its namespace; a topic var ``enum`` naming no ``<namespace>::<enum>``, with a
  negative code or a code longer than its ``max_len``, and an ``enum`` on a placeholder; an
  enum, outcome or topic var enum constant that is not a C identifier, longer than 63
  characters, or named twice; a write rate that is not ``"<n>/s|min|h|day"`` within u32 per day,
  and a ``rate_burst`` (on a key or a command type) without a ``write_rate`` or of 0.
* **Struct value headers:** a header on a key that is not a struct, an empty header, a field type
  other than bool and the 8-, 16- and 32-bit integers, a field name that is not a C identifier or
  is repeated, a header of ``max_size`` bytes or more, and a generated name longer than 63
  characters or produced twice.
* **Variants:** an undeclared ``--variant`` or variant named anywhere; an enum variant that
  removes an unknown name or makes a code, name or string ambiguous.
* **Rights and principals:** grants on unknown keys or scopes, a secret's reader that is not a
  principal, a heartbeat with a second writer, duplicate or reserved principal names.
* **Error groups:** a non-bool or non-STATE error key, a bit outside the width or reserved, two
  keys on one bit, overlapping wire ids.
* **Shadow:** a malformed path, two rows on one path in one document, a shadowed SETTINGS key
  without an applied twin, an enum wire string missing, an unknown rule missing or misplaced, a
  form or scope on the wrong role, a forced key without a delta path, a full document above
  8192 bytes.
* **Topics and payloads:** a placeholder outside the set, a duplicate id, template, order or
  wrapper, an ack that is not a pub topic, a payload member on a sub topic, without exactly one
  source, on an unknown or binary key, or with a misplaced form or unknown rule.
* **Command rows:** an unknown topic or command, a request selected twice on one variant, a
  selector with no or several forms, a field that is not dotted identifiers, an argument key
  outside the command's class that is not a STATE key, an argument value outside its key's enum
  or type, a misplaced encoding, an ``answer`` or ``internal`` with a command or args, ``also``
  without a command, and a command of the variant neither on the wire nor internal.
* **Log streams:** a stream name used twice or not a lower-case identifier; file levels outside
  the RAM levels; a ``ctl_key`` that is not a key of the variant or that no principal may write;
  a ``drop_counter_key`` that is not an unsigned integer key; a boot backing outside the
  variant's allowed backings, or a per-variant entry for an undeclared variant; a
  ``max_line_bytes`` of 0, above ``ram_cap_bytes`` when the stream may be enabled, or above
  ``flash_seg_bytes`` or ``flash_rate_burst_bytes`` when it may write flash; a flash stream with
  a segment size, segment count or byte rate of 0; an ``introduced_in`` later than the contract;
  a sum above its ``[bounds]`` log budget, or a missing budget the streams need.
* **Versions:** anything a minor may not change (see Versions).
