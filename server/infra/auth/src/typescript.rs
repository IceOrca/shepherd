use ts_rs::{Config, TS};

use crate::{
    account::{
        AccountPermission, AccountStatus, AccountSummary, AuthorizationCatalog, PermissionEffect, PermissionSummary,
        Role, RoleSummary,
    },
    dto::{
        AccessClaims, AuthProfileResponse, AuthRequest, AuthResponse, ChangePasswordRequest,
        InvalidCredentialsResponse, MessageResponse, RegisterUserRequest, ResetPasswordRequest,
        UpdateAccountPermissionsRequest, UpdateAccountRolesRequest, UpdateAccountStatusRequest,
    },
};

pub fn contract() -> String {
    let config = Config::new().with_large_int("number");
    let mut output = String::new();

    push::<Role>(&mut output, &config);
    push::<AccountStatus>(&mut output, &config);
    push::<PermissionEffect>(&mut output, &config);
    push::<AccountPermission>(&mut output, &config);
    push::<AccountSummary>(&mut output, &config);
    push::<RoleSummary>(&mut output, &config);
    push::<PermissionSummary>(&mut output, &config);
    push::<AuthorizationCatalog>(&mut output, &config);
    push::<AuthRequest>(&mut output, &config);
    push::<AuthResponse>(&mut output, &config);
    push::<AccessClaims>(&mut output, &config);
    push::<RegisterUserRequest>(&mut output, &config);
    push::<MessageResponse>(&mut output, &config);
    push::<AuthProfileResponse>(&mut output, &config);
    push::<UpdateAccountStatusRequest>(&mut output, &config);
    push::<UpdateAccountRolesRequest>(&mut output, &config);
    push::<UpdateAccountPermissionsRequest>(&mut output, &config);
    push::<ChangePasswordRequest>(&mut output, &config);
    push::<ResetPasswordRequest>(&mut output, &config);
    push::<InvalidCredentialsResponse>(&mut output, &config);

    output
}

fn push<T: TS>(output: &mut String, config: &Config) {
    output.push_str("export ");
    output.push_str(&T::decl(config));
    output.push_str("\n\n");
}
