use std::path::{Path, PathBuf};
use anyhow::{Result, bail};
use crate::shell;

fn gen_file(dest:&PathBuf, content: &String, force: bool) -> Result<()> {
    if ! Path::new(dest).is_file() || force {
        match std::fs::write(dest, content) {Ok(_) => {}, Err(e) => bail!("Error {} while generating: {}", e, dest.display()),};
    }
    Ok(())
}

pub fn run_init(src: &PathBuf) -> Result<()> {
    shell::run_log(&format!("cd {:?};terraform init", src))
}

pub fn run_plan(src: &PathBuf) -> Result<()> {
    // Check if "init" need to be run
    let mut file = PathBuf::new();
    file.push(src);
    file.push(".terraform.lock.hcl");
    if ! Path::new(&file).is_file() {
        match run_init(src) {Ok(_) => {}, Err(e) => {return Err(e)}}
    }
    let mut file = PathBuf::new();
    file.push(src);
    file.push("env.tfvars");
    if ! Path::new(&file).is_file() {
        bail!("`env.tfvars` should be there");
    }
    shell::run_log(&format!("cd {:?};terraform plan -input=false -out=tf.plan -var-file=env.tfvars", src))
}

pub fn run_apply(src: &PathBuf) -> Result<()> {
    let mut file = PathBuf::new();
    file.push(src);
    file.push("tf.plan");
    if ! Path::new(&file).is_file() {
        match run_plan(src) {Ok(_) => {}, Err(e) => {return Err(e)}}
    }
    shell::run_log(&format!("cd {:?};terraform apply -input=false -auto-approve tf.plan", src))
}

pub fn get_plan(src: &PathBuf) -> Result<serde_json::Map<String, serde_json::Value>> {
    let mut file = PathBuf::new();
    file.push(src);
    file.push("tf.plan");
    if ! Path::new(&file).is_file() {
        match run_plan(src) {Ok(_) => {}, Err(e) => {return Err(e)}}
    }
    let output = match shell::get_output(&format!("cd {:?};terraform show -json tf.plan", src)) {Ok(d) => d, Err(e) => {bail!("{e}")}};
    let json: serde_json::Map<String, serde_json::Value> = serde_json::from_str(output.as_str()).unwrap();
    Ok(json)
}

pub fn run_destroy(src: &PathBuf) -> Result<()> {
    // Check if "init" need to be run
    let mut file = PathBuf::new();
    file.push(src);
    file.push(".terraform.lock.hcl");
    if ! Path::new(&file).is_file() {
        match run_init(src) {Ok(_) => {}, Err(e) => {return Err(e)}}
    }
    let mut file = PathBuf::new();
    file.push(src);
    file.push("env.tfvars");
    if ! Path::new(&file).is_file() {
        bail!("`env.tfvars` should be there");
    }
    shell::run_log(&format!("cd {:?};terraform apply -destroy -input=false -auto-approve -var-file=env.tfvars", src))
}

pub fn gen_providers(dest_dir: &PathBuf) -> Result<()> {
    let mut file  = PathBuf::new();
    file.push(dest_dir);
    file.push("providers.tf");
    gen_file(&file, &"
terraform {
  required_providers {
    kustomization = {
        source  = \"kbst/kustomization\"
        version = \"~> 0.9.2\"
    }
#    authentik = {
#        source = \"goauthentik/authentik\"
#        version = \"~> 2023.3.0\"
#    }
#    kubernetes = {
#        source = \"hashicorp/kubernetes\"
#        version = \"~> 2.16.0\"
#    }
  }
}
provider \"kustomization\" {
    kubeconfig_incluster = true
}
#provider \"kubernetes\" {}
#provider \"authentik\" {}
".to_string(), false)
}

pub fn gen_variables(dest_dir: &PathBuf, config:&serde_json::Map<String, serde_json::Value>) -> Result<()> {
  let mut file  = PathBuf::new();
  file.push(dest_dir);
  file.push("variables.tf");

  let mut content  = "
variable \"common_labels\" {
  description = \"Labels to add to every objects_\"
  type        = map
  default     = {}
}
variable \"common_annotations\" {
  description = \"Annotations to add to every objects\"
  type        = map
  default     = {}
}
".to_string();
  for (name,value) in config {
      let str = serde_json::to_string(value).unwrap();
      let output = match shell::get_output(&format!("echo 'jsondecode({:?})'|terraform console",str))  {Ok(d) => d, Err(e) => {bail!("{e}")}};
      log::debug!("{}={}", name, output);
      content += format!("variable \"{}\" {{
  default     = {}
}}
", name, output).as_str();
  }
  gen_file(&file, &content, false)
}

pub fn gen_tfvars(dest_dir: &PathBuf, config:&serde_json::Map<String, serde_json::Value>) -> Result<()> {
    let mut file  = PathBuf::new();
    file.push(dest_dir);
    file.push("env.tfvars");

    let mut content: String = String::new();
    for (name,value) in config {
        let str = serde_json::to_string(value).unwrap();
        let output = match shell::get_output(&format!("echo 'jsondecode({:?})'|terraform console",str))  {Ok(d) => d, Err(e) => {bail!("{e}")}};
        log::debug!("{}={}", name, output);
        content += format!("{} = {}
", name, output).as_str();
    }
    gen_file(&file, &content, true)
}

pub fn gen_datas(dest_dir: &PathBuf) -> Result<()> {
    let mut file  = PathBuf::new();
    file.push(dest_dir);
    file.push("datas.tf");
    gen_file(&file, &"
data \"kustomization_overlay\" \"data\" {
  namespace = var.namespace
  resources = [ for file in fileset(path.module, \"*.yaml\"): file if file != \"index.yaml\"]
}
".to_string(), false)
}

pub fn gen_ressources(dest_dir: &PathBuf) -> Result<()> {
    let mut file  = PathBuf::new();
    file.push(dest_dir);
    file.push("ressources.tf");
    gen_file(&file, &"
# first loop through resources in ids_prio[0]
resource \"kustomization_resource\" \"pre\" {
  for_each = data.kustomization_overlay.data.ids_prio[0]

  manifest = (
    contains([\"_/Secret\"], regex(\"(?P<group_kind>.*/.*)/.*/.*\", each.value)[\"group_kind\"])
    ? sensitive(data.kustomization_overlay.data.manifests[each.value])
    : data.kustomization_overlay.data.manifests[each.value]
  )
}

# then loop through resources in ids_prio[1]
# and set an explicit depends_on on kustomization_resource.pre
# wait 2 minutes for any deployment or daemonset to become ready
resource \"kustomization_resource\" \"main\" {
  for_each = data.kustomization_overlay.data.ids_prio[1]

  manifest = (
    contains([\"_/Secret\"], regex(\"(?P<group_kind>.*/.*)/.*/.*\", each.value)[\"group_kind\"])
    ? sensitive(data.kustomization_overlay.data.manifests[each.value])
    : data.kustomization_overlay.data.manifests[each.value]
  )
  wait = true
  timeouts {
    create = \"5m\"
    update = \"5m\"
  }

  depends_on = [kustomization_resource.pre]
}

# finally, loop through resources in ids_prio[2]
# and set an explicit depends_on on kustomization_resource.main
resource \"kustomization_resource\" \"post\" {
  for_each = data.kustomization_overlay.data.ids_prio[2]

  manifest = (
    contains([\"_/Secret\"], regex(\"(?P<group_kind>.*/.*)/.*/.*\", each.value)[\"group_kind\"])
    ? sensitive(data.kustomization_overlay.data.manifests[each.value])
    : data.kustomization_overlay.data.manifests[each.value]
  )

  depends_on = [kustomization_resource.main]
}
".to_string(), false)
}