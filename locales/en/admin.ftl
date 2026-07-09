# Admin surface: the unassigned-projects page (templates/admin_unassigned.html).
# Reuses nav-project for the "Project" column header. This page has a
# byte-identical snapshot test (admin.rs snapshot_tests); values here must match
# the original literals exactly. Separator glyphs (›, -) stay literal.
admin-unassigned-title = Unassigned Projects - Stackpit
admin-breadcrumb-admin = Admin
admin-unassigned-heading = Unassigned Projects
admin-unassigned-subtitle = Projects auto-registered without an org. Reassign them to a real org.
admin-unassigned-empty = No unassigned projects. All projects belong to a real org.
admin-col-source = Source
admin-col-assign = Assign to org
admin-id-prefix = id:
admin-no-orgs = No orgs available
admin-assign = Assign

# Commercial license page (templates/admin_license.html).
license-watermark = Licensed to
license-page-title = License
license-intro = Commercial-tier license for this installation. Verified locally and offline, no data leaves the server.
license-status = Status
license-customer = Customer
license-contact = Contact
license-issued = Issued
license-expires = Expires
license-lifetime = Lifetime
license-max-orgs = Max organizations
license-unlocked-features = Unlocked features
license-none = None
license-no-license = No commercial license is active. This build runs the open-source tier.
license-activate-heading = Activate license
license-activate-intro = Paste the license blob you received from sales. Activation runs locally, the signature is verified against this binary's baked-in public key. Nothing is sent over the network.
license-key-label = License key
license-key-placeholder = Paste your base64 license key here...
license-activate = Activate
license-replace = Replace license
license-deactivate = Deactivate
license-back = Back to admin
