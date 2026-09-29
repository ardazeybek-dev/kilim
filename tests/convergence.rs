use kilim::{Doc, Op, Patch, VersionVector};

#[test]
fn local_edits() {
    let mut d = Doc::new(1);
    d.insert(0, "helo");
    d.insert(3, "l");
    assert_eq!(d.text(), "hello");
    d.delete(0, 1);
    assert_eq!(d.text(), "ello");
    d.insert(99, "!"); // clamped to the end
    assert_eq!(d.text(), "ello!");
}

#[test]
fn concurrent_inserts_at_same_position_converge() {
    let mut a = Doc::new(1);
    let mut b = Doc::new(2);
    b.apply_all(a.insert(0, "ab"));

    let from_a = a.insert(1, "X");
    let from_b = b.insert(1, "Y");
    a.apply_all(from_b);
    b.apply_all(from_a);

    assert_eq!(a.text(), b.text());
    assert_eq!(a.text().len(), 4);
}

#[test]
fn concurrent_words_do_not_interleave() {
    let mut a = Doc::new(1);
    let mut b = Doc::new(2);
    let from_a = a.insert(0, "hello");
    let from_b = b.insert(0, "world");
    a.apply_all(from_b.clone());
    b.apply_all(from_a.clone());
    assert_eq!(a.text(), b.text());
    assert!(
        a.text() == "helloworld" || a.text() == "worldhello",
        "{}",
        a.text()
    );
}

#[test]
fn delete_and_insert_race() {
    let mut a = Doc::new(1);
    let mut b = Doc::new(2);
    b.apply_all(a.insert(0, "abc"));

    let del = a.delete(1, 1); // a removes "b"
    let ins = b.insert(2, "Z"); // b types right after "b"
    a.apply_all(ins);
    b.apply_all(del);

    assert_eq!(a.text(), "aZc");
    assert_eq!(b.text(), "aZc");
}

#[test]
fn out_of_order_and_duplicate_delivery() {
    let mut a = Doc::new(1);
    let mut ops = a.insert(0, "kilim");
    ops.extend(a.delete(0, 1));

    let mut b = Doc::new(2);
    let mut shuffled: Vec<Op> = ops.iter().rev().cloned().collect();
    shuffled.extend(ops.iter().cloned()); // every op twice
    b.apply_all(shuffled);

    assert_eq!(b.text(), "ilim");
    assert_eq!(b.pending_len(), 0);
}

#[test]
fn offline_replica_catches_up_with_version_vector() {
    let mut a = Doc::new(1);
    let mut b = Doc::new(2);
    b.apply_all(a.insert(0, "shared "));

    // b goes offline; both keep typing.
    let _ = a.insert(7, "from a");
    let _ = b.insert(0, ">> ");

    let for_b = a.ops_since(b.version());
    let for_a = b.ops_since(a.version());
    assert_eq!(for_b.len(), 6, "only the missing ops are sent");
    a.apply_all(for_a);
    b.apply_all(for_b);
    assert_eq!(a.text(), ">> shared from a");
    assert_eq!(a.text(), b.text());
    assert!(a.ops_since(b.version()).is_empty());
}

#[test]
fn patches_describe_visible_changes() {
    let mut a = Doc::new(1);
    let mut b = Doc::new(2);
    let ops = a.insert(0, "hi");
    assert_eq!(
        b.apply_all(ops),
        vec![
            Patch::Insert { index: 0, ch: 'h' },
            Patch::Insert { index: 1, ch: 'i' }
        ]
    );
    assert_eq!(
        b.apply_all(a.delete(0, 1)),
        vec![Patch::Delete { index: 0 }]
    );
}

#[test]
fn anchors_follow_remote_edits() {
    let mut a = Doc::new(1);
    let mut b = Doc::new(2);
    b.apply_all(a.insert(0, "world"));
    let cursor = b.anchor(5); // b's cursor at the end

    b.apply_all(a.insert(0, "hello "));
    assert_eq!(b.resolve(cursor), 11);

    b.apply_all(a.delete(9, 2)); // removes "ld", including the anchor char
    assert_eq!(b.text(), "hello wor");
    assert_eq!(b.resolve(cursor), 9);
}

#[test]
fn unicode_chars() {
    let mut a = Doc::new(1);
    a.insert(0, "çğış🧶");
    a.delete(4, 1);
    assert_eq!(a.text(), "çğış");
}

#[test]
fn ops_roundtrip_through_json() {
    let mut a = Doc::new(1);
    let ops = a.insert(0, "json");
    let wire = serde_json::to_string(&ops).unwrap();
    let back: Vec<Op> = serde_json::from_str(&wire).unwrap();
    assert_eq!(ops, back);

    let v: VersionVector =
        serde_json::from_str(&serde_json::to_string(a.version()).unwrap()).unwrap();
    assert_eq!(&v, a.version());
}

/// Small deterministic PRNG so the fuzz test needs no extra dependency.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Three replicas edit randomly while the "network" delays, reorders and
/// duplicates messages. After everything is delivered all replicas must agree.
#[test]
fn random_edits_over_a_hostile_network_converge() {
    const ALPHABET: &[char] = &['a', 'b', 'c', 'ş', '🧶', ' '];
    for seed in 1..=300u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut docs: Vec<Doc> = (1..=3).map(Doc::new).collect();
        // In-flight messages: (destination, op)
        let mut network: Vec<(usize, Op)> = Vec::new();

        for _ in 0..60 {
            let who = rng.below(3);
            let len = docs[who].len();
            let ops = if len > 0 && rng.below(3) == 0 {
                let pos = rng.below(len);
                docs[who].delete(pos, 1 + rng.below(3))
            } else {
                let ch = ALPHABET[rng.below(ALPHABET.len())];
                docs[who].insert(rng.below(len + 1), &ch.to_string())
            };
            for op in ops {
                for to in (0..3).filter(|&to| to != who) {
                    network.push((to, op.clone()));
                    if rng.below(5) == 0 {
                        network.push((to, op.clone())); // duplicate
                    }
                }
            }
            // Deliver a random subset, in random order.
            for _ in 0..rng.below(network.len() + 1) {
                let (to, op) = network.swap_remove(rng.below(network.len()));
                docs[to].apply(op);
            }
        }
        while !network.is_empty() {
            let (to, op) = network.swap_remove(rng.below(network.len()));
            docs[to].apply(op);
        }

        let text = docs[0].text();
        for d in &docs {
            assert_eq!(d.text(), text, "seed {seed} diverged");
            assert_eq!(d.pending_len(), 0, "seed {seed} stuck ops");
        }
    }
}
