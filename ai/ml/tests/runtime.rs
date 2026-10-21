//! The model registry and jobs, on the chaser's toy policy (sdk/react/bench/ml/chaser/media/policy.onnx, 487 bytes) and,
//! when it's in the model cache, Silero VAD: inputs and outputs inspected, the same outputs for the same inputs, a job's
//! result equal to a run's, one session per model, release.

use std::path::PathBuf;

use raea_ml::{DType, Options, Tensor};

fn policy() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sdk/react/bench/ml/chaser/media/policy.onnx")
}

#[test]
fn policy_runs_deterministically() {
    let m = raea_ml::load(&policy(), &Options::default()).unwrap();
    assert_eq!(m.inputs.len(), 1);
    assert_eq!(m.inputs[0].name, "obs");
    assert_eq!(m.inputs[0].dtype, "float32");
    assert_eq!(m.inputs[0].shape, vec![1, 4]);
    assert_eq!(m.outputs[0].name, "action");
    assert!(m.deterministic());
    let obs = vec![("obs".to_string(), Tensor::f32(vec![1, 4], &[0.5, -0.25, 0.1, 0.0]))];
    let a = m.run(&obs, None).unwrap();
    let b = m.run(&obs, None).unwrap();
    assert_eq!(a, b, "the same inputs, the same outputs");
    let act = a[0].1.as_f32().unwrap();
    assert_eq!(a[0].1.shape, vec![1, 2]);
    // it steers toward the target: +x thrust for a target to the right, -y for one above
    assert!(act[0] > 0.3 && act[1] < -0.2, "{act:?}");
    // a job named by its inputs: asked twice it's one job, and its result is the run's
    let k1 = raea_ml::request(m.clone(), obs.clone());
    let k2 = raea_ml::request(m.clone(), obs.clone());
    assert_eq!(k1, k2);
    assert_eq!(*raea_ml::result(&k1).unwrap(), a);
    assert!(raea_ml::poll(&k1).unwrap().is_ok());
    // one session per model and options
    let again = raea_ml::load(&policy(), &Options::default()).unwrap();
    assert_eq!(again.id, m.id);
    let single = raea_ml::load(&policy(), &Options { threads: Some(1), ..Options::default() }).unwrap();
    assert_ne!(single.id, m.id);
    // and a single thread gives the same answer as four
    assert_eq!(single.run(&obs, None).unwrap(), a);
    raea_ml::release(single.id, true);
    assert!(raea_ml::get(single.id).is_err(), "released at once with now");
    raea_ml::release(again.id, false);
    assert!(raea_ml::get(m.id).is_ok(), "still held once");
    // a wrong input is said plainly
    let e = m.run(&[("x".into(), Tensor::f32(vec![1, 4], &[0.0; 4]))], None).unwrap_err().to_string();
    assert!(e.contains("no input \"x\""), "{e}");
}

#[test]
fn silero_vad_when_cached() {
    let file = raea_ml::models::root().join("silero-vad/silero_vad.onnx");
    if !file.exists() {
        eprintln!("skipped: {} isn't in the model cache", file.display());
        return;
    }
    let m = raea_ml::load(&file, &Options::default()).unwrap();
    let names: Vec<&str> = m.inputs.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["input", "state", "sr"]);
    let tone: Vec<f32> = (0..576).map(|i| (i as f32 * 0.07).sin() * 0.3).collect();
    let feeds = vec![
        ("input".to_string(), Tensor::f32(vec![1, 576], &tone)),
        ("state".to_string(), Tensor::f32(vec![2, 1, 128], &[0.0; 256])),
        ("sr".to_string(), Tensor { dtype: DType::I64, shape: vec![], bytes: 16000i64.to_le_bytes().to_vec() }),
    ];
    let a = m.run(&feeds, None).unwrap();
    assert_eq!(a, m.run(&feeds, None).unwrap());
    let p = a.iter().find(|(n, _)| n == "output").unwrap().1.as_f32().unwrap()[0];
    assert!((0.0..=1.0).contains(&p));
}
