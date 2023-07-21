use std::{
	collections::{BTreeSet, HashSet},
	fs,
	path::Path,
};

use sha1::Digest;

#[tokio::main(flavor = "current_thread")]
async fn main() {
	let credentials = b2creds::Credentials::locate().unwrap();
	let mut auth = b2_client::authorize_account(
		b2_client::client::HyperClient::default(),
		&credentials.application_key_id,
		&credentials.application_key,
	)
	.await
	.unwrap();
	let mut upload_auth_auth = b2_client::authorize_account(
		b2_client::client::HyperClient::default(),
		&credentials.application_key_id,
		&credentials.application_key,
	)
	.await
	.unwrap();
	println!("{auth:?}");
	let mut args = std::env::args();
	args.next().unwrap();
	let folder = args.next().unwrap();
	let bucket = args.next().unwrap();
	let files = walkdir::WalkDir::new(&folder)
		.into_iter()
		.map(Result::unwrap)
		.filter(|entry| entry.file_type().is_file())
		.map(|entry| entry.into_path().strip_prefix(&folder).unwrap().to_owned())
		.collect::<Vec<_>>();
	let files_set = files.iter().map(Path::new).collect::<HashSet<_>>();
	let mut objects: Vec<b2_client::File> = Vec::new();
	let mut file_names_request = b2_client::ListFileNames::builder()
		.bucket_id(&bucket)
		.max_file_count(10000)
		.build()
		.unwrap();
	loop {
		let mut response = b2_client::list_file_names(&mut auth, file_names_request)
			.await
			.unwrap();
		objects.append(&mut response.0);
		if let Some(request) = response.1 {
			file_names_request = request;
		} else {
			break;
		}
	}
	let objects_set = objects
		.iter()
		.map(|file| Path::new(file.file_name()))
		.collect::<HashSet<_>>();
	let mut upload_auth = b2_client::get_upload_authorization_by_id(&mut upload_auth_auth, &bucket)
		.await
		.unwrap();
	for file in &files {
		if !objects_set.contains(Path::new(file)) {
			println!("New file: {}", file.display());
			let content = fs::read(Path::new(&folder).join(file)).unwrap();
			let mut hasher = sha1::Sha1::new();
			hasher.update(&content);
			let sha1 = hasher.finalize();
			b2_client::upload_file(
				&mut upload_auth,
				b2_client::UploadFile::builder()
					.file_name(file.to_str().unwrap())
					.unwrap()
					.content_type("application/json")
					.sha1_checksum(&hex::encode(sha1))
					.build()
					.unwrap(),
				&content,
			)
			.await
			.unwrap();
		}
	}

	for object in &objects {
		if files_set.contains(Path::new(object.file_name())) {
			let content = fs::read(Path::new(&folder).join(object.file_name())).unwrap();
			let mut hasher = sha1::Sha1::new();
			hasher.update(&content);
			let sha1 = hasher.finalize();
			if &*sha1 != &*hex::decode(object.sha1_checksum().unwrap()).unwrap() {
				println!("File changed: {}", object.file_name());

				b2_client::upload_file(
					&mut upload_auth,
					b2_client::UploadFile::builder()
						.file_name(object.file_name())
						.unwrap()
						.content_type("application/json")
						.sha1_checksum(&hex::encode(sha1))
						.build()
						.unwrap(),
					&content,
				)
				.await
				.unwrap();
				b2_client::delete_file_version_by_name_id(
					&mut auth,
					object.file_name(),
					object.file_id(),
					b2_client::BypassGovernance::No,
				)
				.await
				.unwrap();
			}
		}
	}

	for object in objects {
		if !files_set.contains(Path::new(object.file_name())) {
			println!("Deleted file: {}", object.file_name());
			b2_client::delete_file_version(&mut auth, object, b2_client::BypassGovernance::No)
				.await
				.unwrap();
		}
	}
}
