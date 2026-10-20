// `cybermanju-scene` unit tests — multilingual heuristic matching.
//
// These run under `cargo test -p cybermanju-scene`. The TypeScript twin
// (`src/utils/scene.ts`, `tests/frontend/scene.test.ts`) parses the same
// `data/scenes.json`, so both suites pin one shared behavior contract.

use super::*;

fn input(name: &str) -> SceneInput {
    SceneInput {
        file_name: name.to_string(),
        ..Default::default()
    }
}

fn top_category(scores: &[SceneScore]) -> Option<&str> {
    scores.first().map(|s| s.category.as_str())
}

fn score_of(scores: &[SceneScore], category: &str) -> f32 {
    scores
        .iter()
        .find(|s| s.category == category)
        .map(|s| s.score)
        .unwrap_or(0.0)
}

#[test]
fn tables_parse_with_expected_shape() {
    let cats = category_list();
    assert!(
        cats.len() >= 14,
        "expected 14+ categories, got {}",
        cats.len()
    );
    let ids: Vec<&str> = cats.iter().map(|(id, _)| id.as_str()).collect();
    for want in [
        "human", "forest", "beach", "party", "animal", "dog", "cat", "city", "snow",
    ] {
        assert!(ids.contains(&want), "missing category {want}");
    }
}

#[test]
fn beach_matches_in_many_languages() {
    for name in [
        "praia_grande.jpg",        // pt
        "playa_del_carmen.png",    // es
        "plage_normandie.jpg",     // fr
        "ostsee_strand.heic",      // de
        "spiaggia_roma.jpg",       // it
        "scheveningen_strand.jpg", // nl
        "пляж_сочи.jpg",           // ru
        "海滩度假.jpg",            // zh
        "沖縄ビーチ.jpg",          // ja
        "sandy_beach_day.jpg",     // en
    ] {
        let scores = classify(&input(name));
        assert_eq!(
            top_category(&scores),
            Some("beach"),
            "{name} should top beach, got {scores:?}"
        );
        assert!(
            score_of(&scores, "beach") >= 0.5,
            "{name} beach confidence too low: {scores:?}"
        );
    }
}

#[test]
fn forest_party_animal_human_queries() {
    let cases = [
        ("floresta_amazonica.jpg", "forest"),
        ("black_forest_trail.jpg", "forest"),
        ("foret_de_fontainebleau.jpg", "forest"),
        ("festa_junina.jpg", "party"),
        ("birthday_party_2024.jpg", "party"),
        ("結婚式_京都.jpg", "wedding"),
        ("hochzeit_berlin.jpg", "wedding"),
        ("meu_cachorro.jpg", "dog"),
        ("gato_no_telhado.jpg", "cat"),
        ("птица_в_парке.jpg", "bird"),
        ("portrait_maria.jpg", "human"),
        ("城市夜景.jpg", "city"),
        ("schnee_winter.jpg", "snow"),
    ];
    for (name, want) in cases {
        let scores = classify(&input(name));
        assert!(
            scores.iter().any(|s| s.category == want),
            "{name} should report {want}, got {scores:?}"
        );
    }
}

#[test]
fn query_matching_powers_dynamic_search() {
    for (q, want) in [
        ("praia", "beach"),
        ("playa", "beach"),
        ("plage", "beach"),
        ("strand", "beach"),
        ("festa", "party"),
        ("cachorro", "dog"),
        ("gato", "cat"),
        ("floresta", "forest"),
        ("neve", "snow"),
        ("casamento", "wedding"),
    ] {
        let scores = match_query(q);
        assert!(
            scores.iter().any(|s| s.category == want),
            "query {q} should match {want}, got {scores:?}"
        );
    }
}

#[test]
fn more_evidence_ranks_higher() {
    let one = classify(&input("beach.jpg"));
    let two = classify(&SceneInput {
        file_name: "beach_party.jpg".to_string(),
        tags: vec!["praia".to_string(), "sunset".to_string()],
        ..Default::default()
    });
    assert!(score_of(&two, "beach") >= score_of(&one, "beach"));
    assert!(two.iter().any(|s| s.category == "sunset"));
}

#[test]
fn tags_alone_can_carry_a_match() {
    let scores = classify(&SceneInput {
        file_name: "IMG_0042.jpg".to_string(),
        tags: vec!["praia".to_string(), "família".to_string()],
        ..Default::default()
    });
    assert!(scores.iter().any(|s| s.category == "beach"));
}

#[test]
fn child_categories_imply_parents() {
    let scores = classify(&input("meu_cachorro_rex.jpg"));
    assert!(scores.iter().any(|s| s.category == "dog"));
    let animal = score_of(&scores, "animal");
    assert!(animal > 0.0, "dog should imply animal, got {scores:?}");
    assert!(animal < score_of(&scores, "dog"));
}

#[test]
fn face_cluster_boosts_human() {
    let plain = classify(&input("IMG_0001.jpg"));
    assert!(
        plain.is_empty(),
        "hash-named file must stay silent: {plain:?}"
    );
    let with_faces = classify(&SceneInput {
        file_name: "IMG_0001.jpg".to_string(),
        has_faces: true,
        ..Default::default()
    });
    assert_eq!(top_category(&with_faces), Some("human"));
}

#[test]
fn empty_and_garbage_stay_silent() {
    for name in ["", "IMG_0042.jpg", "DSC_9918.heic", "document.pdf", "a.jpg"] {
        let scores = classify(&input(name));
        assert!(
            scores.is_empty(),
            "{name} must report nothing, got {scores:?}"
        );
    }
}

#[test]
fn norm_folds_diacritics() {
    assert_eq!(norm("PÔR DO SOL"), "por do sol");
    assert_eq!(norm("Forêt_D'Été"), "foret_d'ete");
    assert_eq!(norm("Straße"), "strase"); // ß folds lossy-single by design
    assert_eq!(norm("пляЖ"), "пляж");
}

#[test]
fn every_category_is_reachable_in_english() {
    // Each category must fire on at least one of its own English keywords,
    // otherwise the table row is dead weight.
    for (id, _label) in category_list() {
        let t = tables();
        let cat = t.categories.iter().find(|c| c.id == id).unwrap();
        let kws = &cat.keywords["en"];
        let hit = kws.iter().any(|kw| {
            classify(&input(&format!("{kw}_photo.jpg")))
                .iter()
                .any(|s| s.category == id)
        });
        assert!(
            hit,
            "category {id} unreachable via its own English keywords"
        );
    }
}
